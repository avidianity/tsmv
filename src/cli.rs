use clap::{Parser, Subcommand};
use std::path::PathBuf;

use tsmv::options::MoveOptions;

/// Safely move TypeScript files/folders and update imports
#[derive(Parser)]
#[command(name = "tsmv", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Recursively move directories
    #[arg(short = 'r', long, global = true)]
    pub recursive: bool,

    /// Prompt before overwrite
    #[arg(short = 'i', long, global = true)]
    pub interactive: bool,

    /// Force overwrite without prompt
    #[arg(short = 'f', long, global = true)]
    pub force: bool,

    /// Show what would be moved without making changes
    #[arg(short = 'n', long, global = true)]
    pub dry_run: bool,

    /// Display detailed operation logs
    #[arg(short = 'v', long, global = true)]
    pub verbose: bool,

    /// File extensions to consider (comma-separated, default: .ts,.tsx)
    #[arg(long, global = true, default_value = ".ts,.tsx")]
    pub extensions: String,

    /// Path to tsconfig.json
    #[arg(long, global = true)]
    pub tsconfig: Option<PathBuf>,

    /// Convert relative imports to absolute imports (default: true)
    #[arg(long = "absolute-imports", global = true, action = clap::ArgAction::SetTrue, default_value_t = true)]
    pub absolute_imports: bool,

    /// Disable conversion to absolute imports
    #[arg(long = "no-absolute-imports", global = true, action = clap::ArgAction::SetTrue, overrides_with = "absolute_imports")]
    pub no_absolute_imports: bool,

    /// Alias prefix for absolute imports
    #[arg(long, global = true, default_value = "@")]
    pub alias_prefix: String,

    /// Print the LLM-optimized usage guide and exit
    #[arg(long = "usage-llm")]
    pub usage_llm: bool,

    /// Source file(s) followed by destination (last argument is destination)
    #[arg(required = false, num_args = 2..)]
    pub args: Vec<String>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Move TypeScript files/folders and update imports
    Move {
        /// Source file(s) followed by destination (last argument is destination)
        #[arg(required = true, num_args = 2..)]
        args: Vec<String>,
    },
    /// Generate shell completions
    GenerateCompletions {
        /// Shell to generate completions for
        #[arg(value_name = "SHELL")]
        shell: String,
    },
    /// Update tsmv to the latest published release
    #[command(visible_alias = "update")]
    SelfUpdate {
        /// Reinstall even if already on the latest version
        #[arg(long)]
        force: bool,
    },
    /// Remove the installed tsmv binary
    #[command(visible_alias = "uninstall")]
    SelfUninstall {
        /// Do not prompt for confirmation
        #[arg(short = 'y', long)]
        yes: bool,
    },
}

impl Cli {
    pub fn into_move_options(&self) -> MoveOptions {
        MoveOptions {
            recursive: self.recursive,
            interactive: self.interactive,
            force: self.force,
            dry_run: self.dry_run,
            verbose: self.verbose,
            extensions: MoveOptions::parse_extensions(&self.extensions),
            tsconfig: self.tsconfig.clone(),
            absolute_imports: self.absolute_imports && !self.no_absolute_imports,
            alias_prefix: self.alias_prefix.clone(),
        }
    }

    /// Extract (sources, destination) from args or subcommand.
    pub fn extract_args(&self) -> (&[String], &str) {
        match &self.command {
            Some(Commands::Move { args }) => {
                let (sources, dest) = args.split_at(args.len() - 1);
                (sources, dest[0].as_str())
            }
            Some(Commands::GenerateCompletions { .. })
            | Some(Commands::SelfUpdate { .. })
            | Some(Commands::SelfUninstall { .. }) => {
                // Handled in main.rs before this call
                (&[], "")
            }
            None => {
                let (sources, dest) = self.args.split_at(self.args.len().saturating_sub(1));
                (sources, if dest.is_empty() { "" } else { dest[0].as_str() })
            }
        }
    }
}
