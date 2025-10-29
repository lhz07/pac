use crate::{
    PAC_PATH,
    errors::CatError,
    macos::file::add_permit,
    package::{
        load_path::modify_load_path,
        relocate::{relocate_install_prefix, replace_str},
    },
};
use apple_codesign::{CodeSignatureFlags, SigningSettings};
use goblin::mach::Mach;
use std::{
    fs::{self},
    io,
};
use walkdir::WalkDir;

pub fn patch_binary(binary: Vec<u8>, prefix_with_version: &str) -> Result<Vec<u8>, CatError> {
    let mut binary = modify_load_path(binary, prefix_with_version)?;
    relocate_install_prefix(&mut binary, prefix_with_version, PAC_PATH)?;
    Ok(binary)
}

fn is_binary(data: &[u8]) -> bool {
    infer::is(data, "mach")
}

fn is_executable_or_dylib(data: &[u8]) -> bool {
    use goblin::mach::header;
    if let Ok(macho) = Mach::parse(data) {
        match macho {
            Mach::Binary(macho_bin) => {
                let header = &macho_bin.header.filetype;
                match *header {
                    header::MH_EXECUTE | header::MH_DYLIB | header::MH_BUNDLE => true,
                    _ => false,
                }
            }
            Mach::Fat(_) => true,
        }
    } else {
        false
    }
}

pub fn before_install<P>(path: P, prefix_with_version: &str) -> Result<(), CatError>
where
    P: AsRef<std::path::Path>,
{
    let walk = WalkDir::new(path);
    for entry in walk {
        let entry = entry.map_err(|e| -> io::Error { e.into() })?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let relative = path.strip_prefix(std::env::temp_dir()).unwrap().display();
        let data =
            fs::read(path).inspect_err(|e| eprintln!("read {} data error: {e}", relative))?;
        if is_binary(&data) {
            if !is_executable_or_dylib(&data) {
                continue;
            }
            println!("try to patch binary: {}", relative);
            let patched_binary = match patch_binary(data, prefix_with_version) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("Warning: can not patch binary: {} Error: {e}", relative);
                    continue;
                }
            };
            // add user write permission
            add_permit(path, 0o200)?;
            fs::write(path, patched_binary)?;
            // sign the binary
            let mut settings = SigningSettings::default();
            settings.set_binary_identifier(
                apple_codesign::SettingsScope::Main,
                format!("org.pac.binary.{}", path.file_name().unwrap().display()),
            );
            settings.set_code_signature_flags(
                apple_codesign::SettingsScope::Main,
                CodeSignatureFlags::ADHOC | CodeSignatureFlags::ENFORCEMENT,
            );
            let sign = apple_codesign::UnifiedSigner::new(settings);
            if let Err(e) = sign.sign_macho(&path, &path) {
                eprintln!(
                    "Warning: can not sign binary {}, error: {e}\n\
                    You may need to sign it manually with `codesign --sign - --force <path>`",
                    relative
                );
            }
        } else {
            // replace string here
            let content = match String::from_utf8(data) {
                Ok(s) => s,
                Err(_) => {
                    // if it is not a utf-8 file, skip
                    continue;
                }
            };
            let content = replace_str(&content, prefix_with_version, PAC_PATH);
            add_permit(path, 0o200)?;
            fs::write(&path, content)?;
        }
    }
    Ok(())
}
