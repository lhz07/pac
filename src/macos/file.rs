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

pub fn remove_file_force<P: AsRef<Path>>(path: P) -> Result<(), io::Error> {
    if let Err(e) = fs::remove_file(&path)
        && e.kind() == io::ErrorKind::PermissionDenied
    {
        // improve permission
        add_permit(&path, 0o200)?;
        fs::remove_file(&path)?;
    }
    Ok(())
}

pub fn remove_dir_force<P: AsRef<Path>>(path: P) -> Result<(), io::Error> {
    if let Err(e) = fs::remove_dir(&path)
        && e.kind() == io::ErrorKind::PermissionDenied
    {
        // improve permission
        add_permit(&path, 0o200)?;
        fs::remove_dir(&path)?;
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

// FIXME: the logic of handling error of permission denied is weird,
// needs to be improved
pub async fn cp_dir_with_record_and_check<P, Q>(
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
                // so just return an error
                return Err(CatError::Pac(format!(
                    "absolute path symlink is not supported: {} -> {}",
                    entry.path().display(),
                    target.display()
                )));
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
        else if let Err(e) = fs::copy(entry.path(), &dst)
            && e.kind() == io::ErrorKind::PermissionDenied
        {
            // improve permission
            add_permit(&dst, 0o200)?;
            fs::copy(entry.path(), &dst)?;
        }
    }

    Ok(())
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
        if let Err(e) = fs::copy(entry.path(), &dst)
            && e.kind() == io::ErrorKind::PermissionDenied
        {
            // improve permission
            add_permit(&dst, 0o200)?;
            fs::copy(entry.path(), &dst)?;
        }
    }

    Ok(())
}

#[test]
fn test_rm_rf() {
    remove_dir_recursively_force("/var/folders/wc/w_4_gvg16bddwnmlv4jxcqgw0000gn/T/libpng-0e84944536d6bf2c7cfd393a4576acf5c0ced03992d156685a7f83c7d2a60215.tar").unwrap();
}

#[test]
fn test_cp_r() {
    cp_dir("/var/folders/wc/w_4_gvg16bddwnmlv4jxcqgw0000gn/T/fish-7c180ae437fb7c0a71f9135ae87cbfaec7af7f7a7658294071fb3f30bbf456cf.tar/fish/4.1.2", "/opt/pac/test").unwrap();
}
