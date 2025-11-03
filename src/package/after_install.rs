use std::{io, path::Path};

use walkdir::WalkDir;

use crate::macos::file::set_permit;

pub fn set_proper_permissions<P>(path: P, permission: u32) -> Result<(), io::Error>
where
    P: AsRef<Path>,
{
    let walk = WalkDir::new(path);
    for entry in walk {
        let entry = entry.map_err(|e| -> io::Error { e.into() })?;
        if entry.file_type().is_file() || entry.file_type().is_dir() {
            set_permit(entry.path(), permission)?;
        }
    }
    Ok(())
}
