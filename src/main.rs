use std::sync::LazyLock;

use pac::{
    CACHE_DIR,
    brew_api::install_pac,
    macos::version::ARCH_OS,
};

#[tokio::main]
async fn main() {
    LazyLock::force(&ARCH_OS);
    LazyLock::force(&CACHE_DIR);

    let name = std::env::args()
        .nth(1)
        .expect("Please provide a formula name");
    if let Err(e) = install_pac(&name).await {
        eprintln!("{}", e);
    }
}
