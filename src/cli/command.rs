use clap::{ArgGroup, Args, Parser, Subcommand};

#[derive(Parser)]
#[command(name = "pac", version = "0.1.0", about = "A fast package manager")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Install a package
    #[command(group(
        ArgGroup::new("install_source")
            .required(true)
            .args(&["names", "dir"])
    ))]
    Install(InstallArgs),

    /// Uninstall a package
    Uninstall {
        /// Package name to uninstall
        #[arg(help = "Package name")]
        name: String,
    },

    /// List installed packages
    List,

    /// List leave packages
    Leaves,

    /// Show package info
    Info {
        /// Package name
        #[arg(help = "Package name")]
        name: String,
    },
}

#[derive(Args)]
pub struct InstallArgs {
    /// Package names to install from repository
    #[arg(num_args = 1.., help = "Package names to install")]
    pub names: Vec<String>,

    /// Install from local directory
    #[arg(short = 'd', long = "dir", help = "Install from local directory")]
    pub dir: Option<String>,
}
