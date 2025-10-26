use std::path::Path;

use walkdir::WalkDir;

use crate::{
    PAC_PATH,
    database::local::{PacState, SqlTransaction},
    errors::CatError,
    macos::file::{remove_dir_force, remove_file_force},
};

pub async fn uninstall_a_pac(name: &str) -> Result<(), CatError> {
    // find the pac id
    let mut tx = SqlTransaction::new().await?;
    let (id, state) = match tx.is_installed(name).await? {
        Some(s) => s,
        None => {
            println!("Package {} is not installed.", name);
            return Ok(());
        }
    };
    if let PacState::Installed = state {
        tx.update_pac_state(id, PacState::Broken).await?;
        tx.commit().await?;
        tx = SqlTransaction::new().await?;
    }
    // find dependent pacs
    let rev_deps = tx.get_reverse_deps(name).await?;
    if !rev_deps.is_empty() {
        return Err(CatError::Pac(format!(
            "Cannot uninstall package {} because the following packages depend on it:\n{:?}",
            name, rev_deps
        )));
    }
    // find installed files
    let installed_files = tx.get_installed_files(id).await?;
    // remove installed files
    for file in installed_files.iter() {
        if let Err(e) = remove_file_force(&file) {
            if e.kind() != std::io::ErrorKind::NotFound {
                eprintln!("Failed to remove file {:?}: {}", file, e);
                return Err(CatError::Pac(format!("Cannot uninstall package {}", name)));
            }
        }
    }
    // remove pac record from database
    tx.delete_a_pac(id).await?;
    tx.commit().await?;
    println!("Pac {} is removed", name);
    let mut tx = SqlTransaction::new().await?;
    // check orphan deps
    let mut orphan_pacs = tx.get_orphan_pacs().await?;
    while !orphan_pacs.is_empty() {
        for (id, name, state) in orphan_pacs {
            println!("removing orphan pac: {}", name);
            tx = SqlTransaction::new().await?;
            let installed_files = tx.get_installed_files(id).await?;
            if let PacState::Installed = state {
                tx.update_pac_state(id, PacState::Broken).await?;
                tx.commit().await?;
                tx = SqlTransaction::new().await?;
            }
            // remove installed files
            for file in installed_files {
                if let Err(e) = remove_file_force(&file) {
                    if e.kind() != std::io::ErrorKind::NotFound {
                        eprintln!("Failed to remove file {:?}: {}", file, e);
                        return Err(CatError::Pac(format!("Cannot uninstall package {}", name)));
                    }
                }
            }
            tx.delete_a_pac(id).await?;
            tx.commit().await?;
            println!("Pac {} is removed", name);
        }
        tx = SqlTransaction::new().await?;
        orphan_pacs = tx.get_orphan_pacs().await?;
    }

    // clean empty dirs
    let mut walk = WalkDir::new(PAC_PATH)
        .contents_first(true)
        .into_iter()
        .filter_entry(|e| e.file_type().is_dir())
        .collect::<Vec<_>>();
    // skip root
    walk.pop();

    for entry in walk {
        match entry {
            Ok(entry) => {
                if let Err(e) = remove_dir_force(entry.path())
                    && e.kind() != std::io::ErrorKind::DirectoryNotEmpty
                {
                    eprintln!(
                        "Warning: can not remove dir: {}, error: {e}",
                        entry.path().display()
                    )
                }
            }
            Err(e) => {
                eprintln!(
                    "Warning: can not access dir: {}, error: {e}",
                    e.path().unwrap_or_else(|| Path::new("unknown")).display()
                );
                continue;
            }
        }
    }
    Ok(())
}
