use crate::{
    database::{
        basic::{SqlPool, SqlRead},
        local::{PacState, SqlTransMark},
    },
    errors::CatError,
    macos::file::remove_file_force,
    package::clean::{remove_empty_dirs, remove_orphan_pacs},
};

/// database **write**
pub async fn uninstall_a_pac(name: &str, mut mark: SqlTransMark) -> Result<(), CatError> {
    let mut pool = SqlPool;
    // find the pac id
    let (id, state) = match pool.is_installed(name).await? {
        Some(s) => s,
        None => {
            println!("Package {} is not installed.", name);
            return Ok(());
        }
    };
    // find dependent pacs
    let rev_deps = pool.get_reverse_deps(name).await?;
    if !rev_deps.is_empty() {
        return Err(CatError::Pac(format!(
            "Cannot uninstall package {} because the following packages depend on it:\n{:?}",
            name, rev_deps
        )));
    }
    if let PacState::Installed = state {
        let mut tx = mark.into_transaction().await?;
        tx.update_pac_state(id, PacState::Broken).await?;
        mark = tx.commit().await?;
    }
    // find installed files
    let installed_files = pool.get_installed_files(id).await?;
    // remove installed files
    for file in installed_files.iter() {
        if let Err(e) = remove_file_force(file)
            && e.kind() != std::io::ErrorKind::NotFound {
                eprintln!("Failed to remove file {:?}: {}", file, e);
                return Err(CatError::Pac(format!("Cannot uninstall package {}", name)));
            }
    }
    // remove pac record from database
    let mut tx = mark.into_transaction().await?;
    tx.delete_a_pac(id).await?;
    mark = tx.commit().await?;
    println!("Pac {} is removed", name);
    // check orphan deps
    remove_orphan_pacs(mark).await?;

    // clean empty dirs
    remove_empty_dirs()?;
    Ok(())
}
