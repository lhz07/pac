use crate::{
    PAC_PATH,
    errors::CatError,
    macos::file::add_permit,
    package::{load_path::modify_load_path, relocate::relocate_install_prefix},
};
use apple_codesign::{CodeSignatureFlags, SigningSettings};
use goblin::Object;
use std::fs::{self};
use walkdir::WalkDir;

pub fn patch_binary(binary: Vec<u8>, prefix_with_version: &str) -> Result<Vec<u8>, CatError> {
    let mut binary = modify_load_path(binary, prefix_with_version)?;
    relocate_install_prefix(&mut binary, prefix_with_version, PAC_PATH)?;
    Ok(binary)
}

fn is_binary(data: &[u8]) -> bool {
    infer::is(data, "mach")
        && Object::parse(data)
            .map(|obj| matches!(obj, Object::Mach(_)))
            .unwrap_or(false)
}

pub fn before_install<P>(path: P, prefix_with_version: &str) -> Result<(), CatError>
where
    P: AsRef<std::path::Path>,
{
    let walk = WalkDir::new(path).into_iter().filter_map(|e| e.ok());
    for entry in walk {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let relative = path.strip_prefix(std::env::temp_dir()).unwrap().display();
        let data =
            fs::read(path).inspect_err(|e| eprintln!("read {} data error: {e}", relative))?;
        if is_binary(&data) {
            // println!("try to patch binary: {}", relative);
            let patched_binary = match patch_binary(data, prefix_with_version) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("Warning: can not patch binary: {} Error: {e}", relative);
                    continue;
                }
            };
            // add user write permission
            add_permit(path, 0o200)?;
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
            fs::write(path, patched_binary)?;
            let sign = apple_codesign::UnifiedSigner::new(settings);
            sign.sign_macho(&path, &path).unwrap();
        } else {
            // replace string here
        }
    }
    Ok(())
}
