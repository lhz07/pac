use std::{
    collections::HashSet,
    ffi::OsStr,
    fs, io,
    path::{Path, PathBuf},
    sync::LazyLock,
};

use crate::{
    PAC_PATH,
    database::local::SqlTransaction,
    errors::CatError,
    macos::file::{
        add_permit, copy_file_force, cp_dir_patch, cp_dir_with_record_and_check, remove_file_force,
    },
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
    set.insert(".bottle");
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
    let symlinks = cp_dir_with_record_and_check(&path, PAC_PATH, installed_paths, tx).await?;
    // check symlinks
    // if the symbolic link is broken, just copy the
    // original file to dst
    for (dst, target) in symlinks {
        let actual_target = fs::read_link(&dst)?;
        if !fs::exists(&dst)? {
            println!(
                "broken symlink: {} -> {}",
                dst.display(),
                actual_target.display()
            );
            remove_file_force(&dst)?;
            copy_file_force(&target, &dst)?;
            println!("copy file: {} -> {}", target.display(), dst.display())
        }
    }
    // special patches
    if let Some(pac_name) = path.as_ref().parent().and_then(|p| p.file_name()) {
        if pac_name == OsStr::new("ca-certificates") {
            println!("special patch for ca-certificates");
            cp_dir_patch(
                path.as_ref().join("share"),
                Path::new(PAC_PATH).join("etc"),
                installed_paths,
                tx,
            )
            .await?;
        } else if pac_name.to_string_lossy().find("openssl").is_some() {
            println!("special patch for openssl");
            const TARGET: &str = "../ca-certificates/cacert.pem";
            let dst = Path::new(PAC_PATH)
                .join("etc")
                .join(pac_name)
                .join("cert.pem");
            if tx.is_path_exist(&dst).await? {
                return Err(CatError::Pac(format!(
                    "file path conflict: {}",
                    dst.display()
                )));
            }
            installed_paths.push(dst.to_path_buf());
            if let Err(e) = std::os::unix::fs::symlink(TARGET, &dst) {
                match e.kind() {
                    io::ErrorKind::AlreadyExists => {
                        remove_file_force(&dst)?;
                        std::os::unix::fs::symlink(TARGET, &dst)?;
                    }
                    io::ErrorKind::PermissionDenied => {
                        add_permit(&dst.parent().unwrap(), 0o200)?;
                        std::os::unix::fs::symlink(TARGET, &dst)?;
                    }
                    _ => return Err(e.into()),
                }
            }
        }
    }
    Ok(())
}
