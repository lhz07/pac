use clap::{ArgGroup, Args, Parser, Subcommand};

#[derive(Parser)]
#[command(name = "pac", version = "0.1.0", about = "A fast package manager")]
pub struct CliArgs {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Install a package
    #[command(group(
        ArgGroup::new("install_source")
            .required(true)
            .multiple(false)
            .args(&["names", "dir"])
    ))]
    Install(InstallArgs),

    /// Uninstall a package
    Uninstall {
        /// Package name to uninstall
        #[arg(help = "Package name")]
        name: String,
    },

    Clean(CleanArgs),

    /// List installed packages
    List,

    Update,

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

#[derive(Parser)]
#[command(
    group(
        ArgGroup::new("clean_mode")
            .args(["cache", "untracked"])
            .required(true)
            .multiple(false)
    )
)]
pub struct CleanArgs {
    #[arg(short = 'c', long = "cache", help = "Clean package cache")]
    pub cache: bool,

    #[arg(
        short = 'u',
        long = "untracked",
        help = "Clean all untracked files in pac path"
    )]
    pub untracked: bool,
}
