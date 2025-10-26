use crate::{
    BREW_CELLAR, BREW_CELLAR_ACTUAL, BREW_PREFIX, PAC_PATH, errors::CatError, macos::version::ARCH,
};
use arwen::macho::MachoContainer;
use goblin::mach::{Mach, MachO};
use std::{io, path::Path};

fn handle_single_binary(mach: MachO) -> Result<Vec<String>, io::Error> {
    let load_paths = mach.libs.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    if !load_paths.is_empty() {
        Ok(load_paths)
    } else {
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Expected load paths in Mach-O binary",
        ))
    }
}

fn list_lib_path(binary: &[u8]) -> Result<Vec<String>, io::Error> {
    match Mach::parse(&binary).map_err(|e| io::Error::other(e))? {
        goblin::mach::Mach::Binary(bin) => return handle_single_binary(bin),
        goblin::mach::Mach::Fat(bin) => {
            for arch in bin.arches().map_err(|e| io::Error::other(e))? {
                if describe_arch(arch.cputype) == ARCH {
                    let start = arch.offset as usize;
                    let end = start + arch.size as usize;
                    let binary = &binary[start..end];
                    let o = Mach::parse(binary).map_err(|e| io::Error::other(e))?;
                    if let goblin::mach::Mach::Binary(bin) = o {
                        return handle_single_binary(bin);
                    } else {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "Expected Mach-O binary inside fat binary",
                        ));
                    }
                }
            }
        }
    }

    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "Expected load paths in Mach-O binary",
    ))
}

fn describe_arch(cputype: u32) -> &'static str {
    match cputype {
        0x01000007 => "x86_64",
        0x0100000C => "arm64",
        _ => "unknown",
    }
}

pub fn modify_load_path(
    mut binary: Vec<u8>,
    prefix_with_version: &str,
) -> Result<Vec<u8>, CatError> {
    let paths = list_lib_path(&binary)?;
    let prefix_1 = format!("{}/{}", BREW_CELLAR, prefix_with_version);
    let prefix_2 = format!("{}/{}", BREW_CELLAR_ACTUAL, prefix_with_version);
    let mut path_iter = paths.into_iter();
    if let Some(p) = path_iter.next()
        && p != "self"
    {
        if p.contains(BREW_PREFIX) {
            let path = Path::new(&p);
            let file_name = path.file_name().unwrap();
            let new_p = format!("{}/lib/{}", PAC_PATH, file_name.to_string_lossy());
            // println!("new path: {}", new_p);
            let mut macho = MachoContainer::parse(&binary)?;
            macho.change_install_id(&new_p)?;
            binary = macho.data;
        }
        if p.contains(&prefix_1) {
            let new_p = p.replacen(&prefix_1, PAC_PATH, 1);
            // println!("new path with version: {}", new_p);
            let mut macho = MachoContainer::parse(&binary)?;
            macho.change_install_id(&new_p)?;
            binary = macho.data;
        }
        if p.contains(&prefix_2) {
            let new_p = p.replacen(&prefix_2, PAC_PATH, 1);
            // println!("new path with version: {}", new_p);
            let mut macho = MachoContainer::parse(&binary)?;
            macho.change_install_id(&new_p)?;
            binary = macho.data;
        }
    }
    for p in path_iter {
        if p.contains(BREW_PREFIX) {
            let path = Path::new(&p);
            let file_name = path.file_name().unwrap();
            let new_p = format!("{}/lib/{}", PAC_PATH, file_name.to_string_lossy());
            let mut macho = MachoContainer::parse(&binary)?;
            macho.change_install_name(&p, &new_p)?;

            binary = macho.data;
        }
        if p.contains(&prefix_1) {
            let new_p = p.replacen(&prefix_1, PAC_PATH, 1);
            // println!("new path with version: {}", new_p);
            let mut macho = MachoContainer::parse(&binary)?;
            macho.change_install_name(&p, &new_p)?;
            binary = macho.data;
        }
        if p.contains(&prefix_2) {
            let new_p = p.replacen(&prefix_2, PAC_PATH, 1);
            // println!("new path with version: {}", new_p);
            let mut macho = MachoContainer::parse(&binary)?;
            macho.change_install_name(&p, &new_p)?;
            binary = macho.data;
        }
    }

    Ok(binary)
}

#[test]
fn test_list_load_path() {
    use std::fs;
    let b = fs::read("./pcre2/pcre2/10.46/lib/libpcre2-8.0.dylib").unwrap();
    let a = list_lib_path(&b);
    println!("{a:?}");
    let b = fs::read("/bin/ls").unwrap();
    let a = list_lib_path(&b).unwrap();
    println!("{a:?}");
}

#[test]
fn test_modify_load_path() {
    // use crate::BREW_CELLAR;
    // modify_load_path("./fish2", &format!("{BREW_CELLAR}/fish/4.1.2")).unwrap();
}
