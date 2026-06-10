pub mod cli;

use clap::Parser;
use clap_complete::Shell;

use crate::cli::Cli;

/// LLM-optimized usage guide, embedded at compile time so `--usage-llm`
/// works from the installed binary without the source tree present.
const USAGE_LLM: &str = include_str!("../usage.llm.md");

fn main() {
    let cli = Cli::parse();

    // Dump the LLM usage guide and exit before any other handling.
    if cli.usage_llm {
        print!("{USAGE_LLM}");
        return;
    }

    let (sources, destination) = cli.extract_args();

    // Check for generate-completions subcommand first
    if let Some(crate::cli::Commands::GenerateCompletions { shell }) = &cli.command {
        let mut cmd = <Cli as clap::CommandFactory>::command();
        let name = cmd.get_name().to_string();
        clap_complete::generate(
            match shell.as_str() {
                "bash" => Shell::Bash,
                "zsh" => Shell::Zsh,
                "fish" => Shell::Fish,
                "elvish" => Shell::Elvish,
                "powershell" => Shell::PowerShell,
                _ => {
                    eprintln!("Unsupported shell: {shell}");
                    std::process::exit(1);
                }
            },
            &mut cmd,
            &name,
            &mut std::io::stdout(),
        );
        return;
    }

    if sources.is_empty() || destination.is_empty() {
        eprintln!("Error: Please specify one or more source paths and a destination.");
        std::process::exit(1);
    }

    let options = cli.into_move_options();

    if options.verbose {
        eprintln!("Extracted sources: [{}]", sources.join(", "));
        eprintln!("Extracted destination: {destination}");
        eprintln!("DEBUG: moveAction called");
        eprintln!("DEBUG: sources: {sources:?}");
        eprintln!("DEBUG: destination: {destination}");
        eprintln!("DEBUG: options: {options:?}");
    }

    if let Err(e) = tsmv::move_files(sources, destination, &options) {
        eprintln!("Move operation failed: {e}");
        std::process::exit(1);
    }
}
