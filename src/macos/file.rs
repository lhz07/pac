use crate::{database::local::SqlTransaction, errors::CatError, package::install::DIR_TO_INSTALL};
use std::{
    fs, io,
    ops::Deref,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

#[derive(Debug, PartialEq, Eq)]
pub struct CmpPath<P: AsRef<Path> + Eq + PartialEq>(pub P);

impl<P> Deref for CmpPath<P>
where
    P: AsRef<Path> + Eq + PartialEq,
{
    type Target = Path;
    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

impl<P> PartialOrd for CmpPath<P>
where
    P: AsRef<Path> + Eq + PartialEq,
{
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<P> Ord for CmpPath<P>
where
    P: AsRef<Path> + Eq + PartialEq,
{
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let mut a = self.0.as_ref().components();
        let mut b = other.0.as_ref().components();
        loop {
            match (a.next(), b.next()) {
                (Some(_), Some(_)) => {
                    continue;
                }
                (Some(_), None) => return std::cmp::Ordering::Less,
                (None, Some(_)) => return std::cmp::Ordering::Greater,
                (None, None) => {
                    if self.0 == other.0 {
                        return std::cmp::Ordering::Equal;
                    } else {
                        return std::cmp::Ordering::Greater;
                    }
                }
            }
        }
    }
}

pub fn add_permit<P>(file_path: P, permit_code: u32) -> Result<(), io::Error>
where
    P: AsRef<std::path::Path>,
{
    let mut old_permit = fs::metadata(&file_path)?.permissions();
    let old_mode = old_permit.mode();
    // add permission
    let mode = old_mode | permit_code;
    if mode == old_mode {
        // no need to do anything
        return Ok(());
    }
    old_permit.set_mode(mode);
    fs::set_permissions(&file_path, old_permit)?;
    Ok(())
}

// FIXME: follow symlink? we should avoid attack of it
pub fn copy_file_force<P, Q>(from: P, to: Q) -> Result<(), io::Error>
where
    P: AsRef<Path>,
    Q: AsRef<Path>,
{
    if let Err(e) = fs::copy(&from, &to) {
        match e.kind() {
            io::ErrorKind::PermissionDenied => {
                // improve permission
                add_permit(&to, 0o200)?;
                if let Some(parent) = to.as_ref().parent() {
                    add_permit(parent, 0o200)?;
                }
                fs::copy(&from, &to)?;
            }
            _ => return Err(e),
        }
    }
    Ok(())
}

pub fn remove_file_force<P: AsRef<Path>>(path: P) -> Result<(), io::Error> {
    // The rm utility removes symbolic links, not the files referenced by the links.
    // So it is safe to use
    if let Err(e) = fs::remove_file(&path) {
        match e.kind() {
            io::ErrorKind::PermissionDenied => {
                // improve permission
                add_permit(&path, 0o200)?;
                fs::remove_file(&path)?;
            }
            _ => return Err(e),
        }
    }
    Ok(())
}

pub fn remove_dir_force<P: AsRef<Path>>(path: P) -> Result<(), io::Error> {
    if let Err(e) = fs::remove_dir(&path) {
        match e.kind() {
            io::ErrorKind::PermissionDenied => {
                // improve permission
                add_permit(&path, 0o200)?;
                fs::remove_dir(&path)?;
            }
            _ => return Err(e),
        }
    }
    Ok(())
}

pub fn remove_dir_recursively_force<P: AsRef<Path>>(path: P) -> Result<(), io::Error> {
    let walk = WalkDir::new(path).contents_first(true);
    for entry in walk {
        let entry = entry?;
        if !entry.file_type().is_dir() {
            remove_file_force(entry.path())?;
        } else {
            remove_dir_force(entry.path())?;
        }
    }
    Ok(())
}

// TODO: decrease code duplicate
pub async fn cp_dir_patch<P, Q>(
    src: P,
    dst: Q,
    installed_paths: &mut Vec<PathBuf>,
    tx: &mut SqlTransaction,
) -> Result<(), CatError>
where
    P: AsRef<Path>,
    Q: AsRef<Path>,
{
    let walk = WalkDir::new(&src).into_iter().filter_entry(|e| {
        // only install dirs at the top level
        if e.depth() == 1 && !e.file_type().is_dir() {
            false
        } else {
            true
        }
    });
    if let Err(e) = fs::create_dir_all(&dst)
        && e.kind() == io::ErrorKind::PermissionDenied
    {
        // improve permission
        add_permit(dst.as_ref().parent().unwrap(), 0o200)?;
        fs::create_dir_all(&dst)?;
    }
    for entry in walk {
        let entry = entry.map_err(|e| -> io::Error { e.into() })?;
        let relative_path = entry
            .path()
            .strip_prefix(&src)
            .map_err(|e| io::Error::other(e))?;
        let dst = dst.as_ref().join(relative_path);
        if tx.is_path_exist(&dst).await? {
            return Err(CatError::Pac(format!(
                "file path conflict: {}",
                dst.display()
            )));
        }
        if !entry.file_type().is_dir() {
            installed_paths.push(dst.to_path_buf());
        }
        if entry.file_type().is_dir() {
            // println!("create dir: {}", dst.display());
            if let Err(e) = fs::create_dir_all(&dst)
                && e.kind() == io::ErrorKind::PermissionDenied
            {
                // improve permission
                add_permit(dst.parent().unwrap(), 0o200)?;
                fs::create_dir_all(&dst)?;
            }
            continue;
        }
        // fs::copy always follow the symlink, so we need to create symlink manually
        // NOTICE: most symlink is relative path
        else if entry.file_type().is_symlink() {
            let target = fs::read_link(entry.path())?;
            if target.is_absolute() {
                // It is too silly to use absolute path for symlink,
                // anyway, let's copy them directly
                fs::copy(&target, &dst)?;
                continue;
            }
            // println!("create link {} -> {}", dst.display(), target.display());
            if let Err(e) = std::os::unix::fs::symlink(&target, &dst) {
                match e.kind() {
                    io::ErrorKind::AlreadyExists => {
                        remove_file_force(&dst)?;
                        std::os::unix::fs::symlink(&target, &dst)?;
                    }
                    io::ErrorKind::PermissionDenied => {
                        add_permit(&dst.parent().unwrap(), 0o200)?;
                        std::os::unix::fs::symlink(&target, &dst)?;
                    }
                    _ => return Err(e.into()),
                }
            }
            continue;
        }
        // println!("copy {} -> {}", entry.path().display(), dst.display());
        else {
            copy_file_force(entry.path(), &dst)?;
        }
    }

    Ok(())
}

// FIXME: the logic of handling error of permission denied is weird,
// needs to be improved
pub async fn cp_dir_with_record_and_check<P, Q>(
    src: P,
    dst: Q,
    installed_paths: &mut Vec<PathBuf>,
    tx: &mut SqlTransaction,
) -> Result<Vec<(PathBuf, PathBuf)>, CatError>
where
    P: AsRef<Path>,
    Q: AsRef<Path>,
{
    let walk = WalkDir::new(&src).into_iter().filter_entry(|e| {
        // only install specific dirs at the top level
        if e.depth() == 1
            && (!DIR_TO_INSTALL.contains(e.file_name().to_string_lossy().as_ref())
                || !e.file_type().is_dir())
        {
            false
        } else {
            true
        }
    });
    let mut symlinks = Vec::new();
    if let Err(e) = fs::create_dir_all(&dst)
        && e.kind() == io::ErrorKind::PermissionDenied
    {
        // improve permission
        add_permit(dst.as_ref().parent().unwrap(), 0o200)?;
        fs::create_dir_all(&dst)?;
    }
    for entry in walk {
        let entry = entry.map_err(|e| -> io::Error { e.into() })?;
        let mut relative_path = entry
            .path()
            .strip_prefix(&src)
            .map_err(|e| io::Error::other(e))?;
        if relative_path.starts_with(".bottle") {
            relative_path = relative_path
                .strip_prefix(".bottle")
                .map_err(|e| io::Error::other(e))?;
        }
        let dst = dst.as_ref().join(relative_path);
        if tx.is_path_exist(&dst).await? {
            return Err(CatError::Pac(format!(
                "file path conflict: {}",
                dst.display()
            )));
        }
        if !entry.file_type().is_dir() {
            installed_paths.push(dst.to_path_buf());
        }
        if entry.file_type().is_dir() {
            // println!("create dir: {}", dst.display());
            if let Err(e) = fs::create_dir_all(&dst)
                && e.kind() == io::ErrorKind::PermissionDenied
            {
                // improve permission
                add_permit(dst.parent().unwrap(), 0o200)?;
                fs::create_dir_all(&dst)?;
            }
            continue;
        }
        // fs::copy always follow the symlink, so we need to create symlink manually
        // NOTICE: most symlink is relative path
        else if entry.file_type().is_symlink() {
            let target = fs::read_link(entry.path())?;
            if target.is_absolute() {
                // It is too silly to use absolute path for symlink,
                // anyway, let's copy them directly
                fs::copy(&target, &dst)?;
                continue;
            }
            // println!("create link {} -> {}", dst.display(), target.display());
            if let Err(e) = std::os::unix::fs::symlink(&target, &dst) {
                match e.kind() {
                    io::ErrorKind::AlreadyExists => {
                        remove_file_force(&dst)?;
                        std::os::unix::fs::symlink(&target, &dst)?;
                    }
                    io::ErrorKind::PermissionDenied => {
                        add_permit(&dst.parent().unwrap(), 0o200)?;
                        std::os::unix::fs::symlink(&target, &dst)?;
                    }
                    _ => return Err(e.into()),
                }
            }
            // convert target to an absolute path
            symlinks.push((dst, entry.path().parent().unwrap().join(target)));
            continue;
        }
        // println!("copy {} -> {}", entry.path().display(), dst.display());
        else {
            copy_file_force(entry.path(), &dst)?;
        }
    }

    Ok(symlinks)
}

pub fn cp_dir_with_record<P, Q>(
    src: P,
    dst: Q,
    installed_paths: &mut Vec<PathBuf>,
) -> Result<(), io::Error>
where
    P: AsRef<Path>,
    Q: AsRef<Path>,
{
    cp_dir_inner::<_, _, true>(src, dst, installed_paths)
}

pub fn cp_dir<P, Q>(src: P, dst: Q) -> Result<(), io::Error>
where
    P: AsRef<Path>,
    Q: AsRef<Path>,
{
    let mut installed_paths = Vec::new();
    cp_dir_inner::<_, _, false>(src, dst, &mut installed_paths)
}

fn cp_dir_inner<P, Q, const RECORD_PATHS: bool>(
    src: P,
    dst: Q,
    installed_paths: &mut Vec<PathBuf>,
) -> Result<(), io::Error>
where
    P: AsRef<Path>,
    Q: AsRef<Path>,
{
    let walk = WalkDir::new(&src).into_iter().filter_entry(|e| {
        // ignore files and hidden folders at the top level
        if e.depth() == 1
            && (e.file_name().to_string_lossy().starts_with(".") || !e.file_type().is_dir())
        {
            false
        } else {
            true
        }
    });
    // FIXME: fix this weird operation, it do not handle error
    // when the error kind is not PermissionDenied
    if let Err(e) = fs::create_dir_all(&dst)
        && e.kind() == io::ErrorKind::PermissionDenied
    {
        // improve permission
        add_permit(dst.as_ref().parent().unwrap(), 0o200)?;
        fs::create_dir_all(&dst)?;
    }
    for entry in walk {
        let entry = entry?;
        let relative_path = entry
            .path()
            .strip_prefix(&src)
            .map_err(|e| io::Error::other(e))?;
        if RECORD_PATHS && !entry.file_type().is_dir() {
            installed_paths.push(relative_path.to_path_buf());
        }
        let dst = dst.as_ref().join(relative_path);
        if entry.file_type().is_dir() {
            // println!("create dir: {}", dst.display());
            if let Err(e) = fs::create_dir_all(&dst)
                && e.kind() == io::ErrorKind::PermissionDenied
            {
                // improve permission
                add_permit(dst.parent().unwrap(), 0o200)?;
                fs::create_dir_all(&dst)?;
            }
            continue;
        }
        // println!("copy {} -> {}", entry.path().display(), dst.display());

        copy_file_force(entry.path(), &dst)?;
    }

    Ok(())
}
