use crate::{
    brew_api::install_pac,
    database::{basic::SqlPool, local::SqlTransMark},
    errors::CatError,
    package::{
        clean::remove_untracked_files,
        list::{list_leaves, list_pacs},
        script::parse::install_pac_from_file,
        uninstall::uninstall_a_pac,
        update::update,
    },
};
use std::path::Path;

pub struct Cli {
    mark: SqlTransMark,
}

impl Cli {
    pub fn new(mark: SqlTransMark) -> Self {
        Self { mark }
    }
    pub async fn remove_untracked_files() -> Result<(), CatError> {
        remove_untracked_files(&mut SqlPool).await
    }

    pub async fn install_pac(self, name: &str) -> Result<(), CatError> {
        install_pac(name, self.mark).await
    }

    pub async fn install_a_pac_from_file(self, path: impl AsRef<Path>) -> Result<(), CatError> {
        install_pac_from_file(path, self.mark).await
    }

    pub async fn uninstall_a_pac(self, name: &str) -> Result<(), CatError> {
        uninstall_a_pac(name, self.mark).await
    }

    pub async fn list_pacs() -> Result<(), CatError> {
        list_pacs(&mut SqlPool).await
    }

    pub async fn list_leaves() -> Result<(), CatError> {
        list_leaves(&mut SqlPool).await
    }

    pub async fn update(self) -> Result<(), CatError> {
        update(self.mark).await
    }
}
