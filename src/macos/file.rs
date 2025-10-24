use std::{fs, io, os::unix::fs::PermissionsExt, path::Path};

use walkdir::WalkDir;

pub fn add_permit<P>(file_path: P, permit_code: u32) -> Result<(), io::Error>
where
    P: AsRef<std::path::Path>,
{
    let mut old_permit = fs::metadata(&file_path)?.permissions();
    let mode = old_permit.mode();
    // add permission
    let mode = mode | permit_code;
    old_permit.set_mode(mode);
    fs::set_permissions(&file_path, old_permit)?;
    Ok(())
}

pub fn remove_dir_force<P: AsRef<Path>>(path: P) -> Result<(), io::Error> {
    let walk = WalkDir::new(path).contents_first(true);
    for entry in walk {
        let entry = entry?;
        if !entry.file_type().is_dir() {
            if let Err(e) = fs::remove_file(entry.path())
                && e.kind() == io::ErrorKind::PermissionDenied
            {
                // improve permission
                add_permit(entry.path(), 0o200)?;
                fs::remove_file(entry.path())?;
            }
        } else {
            if let Err(e) = fs::remove_dir(entry.path())
                && e.kind() == io::ErrorKind::PermissionDenied
            {
                // improve permission
                add_permit(entry.path(), 0o200)?;
                fs::remove_dir(entry.path())?;
            }
        }
    }
    Ok(())
}

pub fn cp_dir<P, Q>(src: P, dst: Q) -> Result<(), io::Error>
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
    remove_dir_force("/var/folders/wc/w_4_gvg16bddwnmlv4jxcqgw0000gn/T/libpng-0e84944536d6bf2c7cfd393a4576acf5c0ced03992d156685a7f83c7d2a60215.tar").unwrap();
}

#[test]
fn test_cp_r() {
    cp_dir("/var/folders/wc/w_4_gvg16bddwnmlv4jxcqgw0000gn/T/fish-7c180ae437fb7c0a71f9135ae87cbfaec7af7f7a7658294071fb3f30bbf456cf.tar/fish/4.1.2", "/opt/pac/test").unwrap();
}
