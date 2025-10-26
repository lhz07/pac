use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "pac", version = "0.1.0", about = "A fast package manager")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Install a package
    Install {
        /// Package name to install
        #[arg(help = "Package name")]
        name: String,
    },

    /// Uninstall a package
    Uninstall {
        /// Package name to uninstall
        #[arg(help = "Package name")]
        name: String,
    },

    /// List installed packages
    List,

    /// Show package info
    Info {
        /// Package name
        #[arg(help = "Package name")]
        name: String,
    },
}
