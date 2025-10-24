use std::{io, path::Path};

use crate::{PAC_PATH, macos::file::cp_dir};

pub fn install<P>(path: P) -> Result<(), io::Error>
where
    P: AsRef<Path>,
{
    // let files = fs::read_dir(&path)?;
    cp_dir(path, PAC_PATH)?;
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
