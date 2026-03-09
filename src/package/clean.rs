use std::path::Path;

use walkdir::WalkDir;

use crate::{
    PAC_PATH,
    database::{
        basic::{SqlPool, SqlRead},
        local::{PacState, SqlTransMark},
    },
    errors::CatError,
    macos::file::{remove_dir_force, remove_file_force},
};

/// database **write**
pub async fn remove_orphan_pacs(mut mark: SqlTransMark) -> Result<(), CatError> {
    // check orphan deps
    let mut pool = SqlPool;
    let mut orphan_pacs = pool.get_orphan_pacs().await?;
    while !orphan_pacs.is_empty() {
        for (id, name, state) in orphan_pacs {
            println!("removing orphan pac: {}", name);
            let installed_files = pool.get_installed_files(id).await?;
            if let PacState::Installed = state {
                let mut tx = mark.into_transaction().await?;
                tx.update_pac_state(id, PacState::Broken).await?;
                mark = tx.commit().await?;
            }
            // remove installed files
            for file in installed_files {
                if let Err(e) = remove_file_force(&file)
                    && e.kind() != std::io::ErrorKind::NotFound {
                        eprintln!("Failed to remove file {:?}: {}", file, e);
                        return Err(CatError::Pac(format!("Cannot uninstall package {}", name)));
                    }
            }
            let mut tx = mark.into_transaction().await?;
            tx.delete_a_pac(id).await?;
            mark = tx.commit().await?;
            println!("Pac {} is removed", name);
        }
        orphan_pacs = pool.get_orphan_pacs().await?;
    }

    Ok(())
}

/// no database
pub fn remove_empty_dirs() -> Result<(), CatError> {
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

/// database read-only
pub async fn remove_untracked_files(pool: &mut impl SqlRead) -> Result<(), CatError> {
    let walk = WalkDir::new(PAC_PATH)
        .into_iter()
        .filter_entry(|e| e.path() != Path::new(PAC_PATH).join("PacData"));
    let tracked_files = pool.get_all_installed_files().await?;
    for entry in walk {
        match entry {
            Ok(entry) => {
                // only skip dirs, handle symlink and files
                if entry.file_type().is_dir() {
                    continue;
                }
                let path = entry.path();
                if tracked_files.contains(path) {
                    // the file is already tracked
                    continue;
                }
                println!("try to remove untracked file: {}", path.display());
                if let Err(e) = remove_file_force(path) {
                    eprintln!(
                        "Warning: can not remove file: {}, error: {e}",
                        entry.path().display()
                    )
                }
            }
            Err(e) => {
                eprintln!(
                    "Warning: can not access file: {}, error: {e}",
                    e.path().unwrap_or_else(|| Path::new("unknown")).display()
                );
                continue;
            }
        }
    }
    Ok(())
}
