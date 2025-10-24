use std::io;

use crate::{BREW_CELLAR, BREW_CELLAR_ACTUAL};

pub fn relocate_install_prefix(
    binary: &mut [u8],
    name_version: &str,
    new_prefix: &str,
) -> Result<(), io::Error> {
    let new_bytes = new_prefix.as_bytes();
    const DEFAULT_BYTES: &[u8] = b"/opt/homebrew";
    let prefix_1 = format!("{}/{}", BREW_CELLAR, name_version);
    let prefix_2 = format!("{}/{}", BREW_CELLAR_ACTUAL, name_version);
    let old_bytes_1 = prefix_1.as_bytes();
    let old_bytes_2 = prefix_2.as_bytes();
    if new_bytes.len() > old_bytes_1.len()
        || new_bytes.len() > old_bytes_2.len()
        || new_bytes.len() > DEFAULT_BYTES.len()
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "New prefix is longer than old prefix or the default prefix",
        ));
    }
    let mut parts = binary.split_mut(|&b| b == 0).collect::<Vec<_>>();
    for s in parts.iter_mut() {
        if s.windows(old_bytes_1.len()).any(|w| w == old_bytes_1) {
            // println!("find bytes to replace new");
            replace_bytes(s, old_bytes_1, new_bytes);
            // println!("{:?}", String::from_utf8_lossy(s));
        }
    }
    for s in parts.iter_mut() {
        if s.windows(old_bytes_2.len()).any(|w| w == old_bytes_2) {
            // println!("find bytes to replace new");
            replace_bytes(s, old_bytes_2, new_bytes);
            // println!("{:?}", String::from_utf8_lossy(s));
        }
    }
    for s in parts.iter_mut() {
        if s.windows(DEFAULT_BYTES.len()).any(|w| w == DEFAULT_BYTES) {
            // println!("find bytes to replace default");
            replace_bytes(s, DEFAULT_BYTES, new_bytes);
            // println!("{:?}", String::from_utf8_lossy(s));
        }
    }
    Ok(())
}

fn replace_bytes(data: &mut [u8], old: &[u8], new: &[u8]) {
    let mut result = Vec::with_capacity(data.len());
    let n = old.len() - new.len();
    let mut i = 0;
    while i < data.len() {
        unsafe {
            if data.get_unchecked(i..).starts_with(old) {
                result.extend_from_slice(new);
                result.extend(std::iter::repeat_n(47, n));
                i += old.len();
            } else {
                result.push(*data.get_unchecked(i));
                i += 1;
            }
        }
    }
    let n = data.len() - result.len();
    result.extend(std::iter::repeat_n(0, n));
    debug_assert!(result.len() == data.len());
    unsafe {
        std::ptr::copy_nonoverlapping(result.as_ptr(), data.as_mut_ptr(), data.len());
    }
}

// #[cfg(not(miri))]
#[test]
fn test_relocate() {
    let old_prefix = "/opt/homebrew/Cellar/fish/4.1.2";
    let new_prefix = "/opt/pac";
    let mut binary = std::fs::read("./fish/fish/4.1.2/bin/fish").unwrap();
    relocate_install_prefix(&mut binary, old_prefix, new_prefix).unwrap();
}

#[test]
fn test_replace_bytes() {
    let old_prefix = "/opt/homebrew/Cellar/fish/4.1.2";
    let new_prefix = "/opt/pac";
    let mut data = "%ls/opt/homebrew/etc/opt/homebrew/Cellar/fish/4.1.2/bin/opt/homebrew/Cellar/fish/4.1.2/share/opt/homebrew/Cellar/fish/4.1.2/share/doc/fishbinshare/fish/private/tmp/fish-20251007-7845-i8mh0h/fish-4.1.2/buildUnexpected directory layout, using compiled-in pathsRunning out of build directory, using paths relative to $CARGO_MANIFEST_DIR (etcshareuser_doc/htmlRunning from relocatable treeshare/doc/fishInvalid executable path '', using compiled-in pathsc
    push/pop not allowed on global stack/opt/homebrew/Cellar/fish/4.1.2".to_string();
    let data_bytes = unsafe { data.as_bytes_mut() };
    replace_bytes(data_bytes, old_prefix.as_bytes(), new_prefix.as_bytes());
    let result = "%ls/opt/homebrew/etc/opt/pac////////////////////////bin/opt/pac////////////////////////share/opt/pac////////////////////////share/doc/fishbinshare/fish/private/tmp/fish-20251007-7845-i8mh0h/fish-4.1.2/buildUnexpected directory layout, using compiled-in pathsRunning out of build directory, using paths relative to $CARGO_MANIFEST_DIR (etcshareuser_doc/htmlRunning from relocatable treeshare/doc/fishInvalid executable path '', using compiled-in pathsc\n    push/pop not allowed on global stack/opt/pac///////////////////////";
    assert_eq!(data, result);
}
