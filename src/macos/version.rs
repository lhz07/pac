use std::sync::LazyLock;

use objc2_foundation::NSProcessInfo;
use strum::{AsRefStr, Display, FromRepr};

pub static ARCH_OS: LazyLock<String> = LazyLock::new(|| match get_version_and_arch() {
    Ok(v) => v,
    Err(e) => {
        eprintln!("{e}");
        std::process::exit(1);
    }
});

pub fn get_version_and_arch() -> Result<String, &'static str> {
    let process_info = NSProcessInfo::processInfo();
    let ver = process_info.operatingSystemVersion();
    let major_ver = ver.majorVersion;
    if major_ver < 0 {
        return Err("Failed to get macOS version");
    }
    let version = MacOSVersion::from_repr(major_ver as usize).ok_or("Unsupported macOS version")?;

    println!("macOS version: macOS {} {}", version, ver.majorVersion);
    Ok(format!(
        "{}_{}",
        ARCH,
        version.to_string().to_ascii_lowercase()
    ))
}
#[cfg(target_arch = "aarch64")]
pub const ARCH: &str = "arm64";
#[cfg(target_arch = "x86_64")]
pub const ARCH: &str = "x86_64";

#[derive(Debug, AsRefStr, FromRepr, Display)]
pub enum MacOSVersion {
    Catalina = 10,
    BigSur,
    Monterey,
    Ventura,
    Sonoma,
    Sequoia,
    Tahoe = 26,
}
