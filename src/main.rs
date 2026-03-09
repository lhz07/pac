use clap::Parser;
use pac::cli::command::{CliArgs, Commands};
use pac::cli::functions::Cli;
use pac::database::local::SqlTransaction;
use pac::{CACHE_DIR, database::local::init_db, macos::version::ARCH_OS};
use std::process::ExitCode;
use std::sync::LazyLock;

#[tokio::main]
async fn main() -> ExitCode {
    LazyLock::force(&ARCH_OS);
    LazyLock::force(&CACHE_DIR);
    if let Err(e) = init_db().await {
        eprintln!("Can not initialize database: {e}");
        return ExitCode::FAILURE;
    }
    let cli_args = CliArgs::parse();
    // SAFETY: there is no other transaction
    let mark = unsafe { SqlTransaction::new_mark() };
    let cli = Cli::new(mark);
    match cli_args.command {
        Commands::Install(args) => match args.dir {
            Some(dir) => {
                println!("Installing from local dir: {}\n", dir);
                if let Err(e) = cli.install_a_pac_from_file(&dir).await {
                    eprintln!("\nCan not install from local dir, error:\n{e}");
                }
            }
            None => {
                let name = args.names.first().unwrap();
                println!("Installing {}\n", name);
                if let Err(e) = cli.install_pac(name).await {
                    eprintln!("\nCan not install {name}, error:\n{e}");
                }
            }
        },
        Commands::Uninstall { name } => {
            println!("Uninstalling {}\n", name);
            if let Err(e) = cli.uninstall_a_pac(&name).await {
                eprintln!("\nCan not finish, encounter an error:\n{e}");
            }
        }
        Commands::Clean(args) => {
            if args.cache {
                println!("cleaning cache...");
                println!("not implemented yet");
            } else if args.untracked {
                println!("cleaning untracked files...");
                if let Err(e) = Cli::remove_untracked_files().await {
                    eprintln!("\nCan not clean untracked files, error:\n{e}");
                }
            }
        }
        Commands::List => {
            if let Err(e) = Cli::list_pacs().await {
                eprintln!("\nCan not list installed packages, error:\n{e}");
            }
        }
        Commands::Leaves => {
            if let Err(e) = Cli::list_leaves().await {
                eprintln!("\nCan not list installed packages, error:\n{e}");
            }
        }
        Commands::Update => {
            if let Err(e) = cli.update().await {
                eprintln!("\nCan not update pacs, error:\n{e}");
            }
        }
        _ => {
            println!("Command not implemented yet.");
        }
    }
    ExitCode::SUCCESS
}
