use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::LazyLock,
};

use crate::{
    PAC_PATH, database::local::SqlTransaction, errors::CatError,
    macos::file::cp_dir_with_record_and_check,
};

pub static DIR_TO_INSTALL: LazyLock<HashSet<&str>> = LazyLock::new(|| {
    let mut set = HashSet::new();
    set.insert("bin");
    set.insert("sbin");
    set.insert("lib");
    set.insert("libexec");
    set.insert("etc");
    set.insert("share");
    set.insert("include");
    set.insert("opt");
    set.insert("var");
    set
});

pub async fn install<P>(
    path: P,
    installed_paths: &mut Vec<PathBuf>,
    tx: &mut SqlTransaction,
) -> Result<(), CatError>
where
    P: AsRef<Path>,
{
    // let files = fs::read_dir(&path)?;
    cp_dir_with_record_and_check(path, PAC_PATH, installed_paths, tx).await?;
    // for entry in files {
    //     let entry = entry?;
    //     // skip files and hidden folders
    //     if entry.file_name().to_string_lossy().starts_with(".") || entry.file_type()?.is_file() {
    //         continue;
    //     }
    //     let opt = fs_extra::dir::CopyOptions::new().overwrite(true);
    //     fs_extra::dir::copy(entry.path(), PAC_PATH, &opt).map_err(|e| io::Error::other(e))?;
    // }
    Ok(())
}
