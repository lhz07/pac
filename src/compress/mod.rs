use std::path::Path;

use infer::MatcherType;

use crate::{compress::zip::unzip, errors::CatError};

pub mod zip;

pub fn decompress_file<P, Q>(from: P, to: Q) -> Result<(), CatError>
where
    P: AsRef<Path>,
    Q: AsRef<Path>,
{
    let file_type = infer::get_from_path(&from)?.ok_or_else(|| {
        CatError::Task(format!(
            "Can not decompress file: {}",
            from.as_ref().display()
        ))
    })?;
    if let MatcherType::Archive = file_type.matcher_type() {
        // decompress archive
        match file_type.extension() {
            "zip" => {
                unzip(&from, &to)?;
            }
            "gz" => {}
            "tar" => {}
            "zst" => {}
            "bz2" => {}
            "xz" => {}
            _ => {
                return Err(CatError::Task(format!(
                    "Unsupported file type: {}",
                    from.as_ref().display()
                )));
            }
        }
    } else {
        // just copy file
        std::fs::copy(from, to)?;
    }

    Ok(())
}

#[test]
fn test_decompress_file() {
    let from = "/Users/lhz/Downloads/v2raya-aarch64-macos";
    let to = "/Users/lhz/Downloads/v2raya-aarch64-macos.d";
    decompress_file(from, to).unwrap();
}
