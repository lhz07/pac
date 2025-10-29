use std::process::ExitCode;
use std::sync::LazyLock;

use clap::Parser;
use pac::cli::command::{Cli, Commands};
use pac::package::list::list_pacs;
use pac::{
    CACHE_DIR, brew_api::install_pac, database::local::init_db, macos::version::ARCH_OS,
    package::uninstall::uninstall_a_pac,
};

#[tokio::main]
async fn main() -> ExitCode {
    LazyLock::force(&ARCH_OS);
    LazyLock::force(&CACHE_DIR);
    if let Err(e) = init_db().await {
        eprintln!("Can not initialize database: {e}");
        return ExitCode::FAILURE;
    }
    let cli = Cli::parse();
    match cli.command {
        Commands::Install { name } => {
            println!("Installing {}\n", name);
            if let Err(e) = install_pac(&name).await {
                eprintln!("\nCan not install {name}, error:\n{e}");
            }
        }
        Commands::Uninstall { name } => {
            println!("Uninstalling {}\n", name);
            if let Err(e) = uninstall_a_pac(&name).await {
                eprintln!("\nCan not finish, encounter an error:\n{e}");
            }
        }
        Commands::List => {
            if let Err(e) = list_pacs().await {
                eprintln!("\nCan not list installed packages, error:\n{e}");
            }
        }
        _ => {
            println!("Command not implemented yet.");
        }
    }
    ExitCode::SUCCESS
}
