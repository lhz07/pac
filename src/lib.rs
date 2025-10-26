use std::{path::PathBuf, sync::LazyLock, time::Duration};

use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::{RetryTransientMiddleware, policies::ExponentialBackoff};

pub mod brew_api;
pub mod cli;
pub mod database;
pub mod errors;
pub mod macos;
pub mod package;
pub mod scopeguard;
pub mod sql;

pub const PAC_PATH: &str = "/opt/pac";
pub const CACHE_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    let mut dir = match dirs::cache_dir() {
        Some(d) => d,
        None => {
            eprintln!("Can not found home dir!");
            std::process::exit(1);
        }
    };
    dir.push("Pac");
    if !dir.exists() {
        println!("{} not exists, try to create it...", dir.display());
        std::fs::create_dir_all(&dir).unwrap_or_else(|e| {
            eprintln!("Can not create cache dir, error: {e}");
            std::process::exit(1);
        });
    }
    dir
});
pub const BREW_PREFIX: &str = "@@HOMEBREW_PREFIX@@";
pub const BREW_CELLAR: &str = "@@HOMEBREW_CELLAR@@";
pub const BREW_CELLAR_ACTUAL: &str = "/opt/homebrew/Cellar";
pub const PC_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/138.0.0.0 Safari/537.36";
pub static CLIENT_WITH_RETRY: LazyLock<ClientWithMiddleware> = LazyLock::new(|| {
    ClientBuilder::new(
        reqwest::Client::builder()
            .user_agent(PC_UA)
            .connect_timeout(Duration::from_secs(10))
            .build()
            .unwrap(),
    )
    .with(RetryTransientMiddleware::new_with_policy(
        ExponentialBackoff::builder().build_with_max_retries(5),
    ))
    .build()
});

pub static BOTTLES_MIRROR: LazyLock<Option<String>> =
    LazyLock::new(|| std::env::var("PAC_BOTTLES_MIRROR").ok());
pub static API_MIRROR: LazyLock<Option<String>> =
    LazyLock::new(|| std::env::var("PAC_API_MIRROR").ok());
