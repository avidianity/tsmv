//! `self-update` and `self-uninstall` for the installed binary.
//!
//! `self-update` reuses the project's own `install.sh` (embedded at build time)
//! so the download/checksum/extract logic lives in exactly one place. The script
//! is pointed at the running binary's directory, replacing it in place.

use std::io::{IsTerminal, Write};
use std::path::Path;
use std::process::Command;

/// The installer is embedded so `self-update` reuses the exact, tested logic
/// without first fetching the script over the network.
const INSTALL_SCRIPT: &str = include_str!("../install.sh");
const REPO: &str = "avidianity/tsmv";

/// Update the running binary to the latest published release. Returns an exit code.
pub fn self_update(force: bool) -> i32 {
    if cfg!(windows) {
        eprintln!(
            "self-update is not supported on Windows; download the latest .zip from \
             https://github.com/{REPO}/releases"
        );
        return 1;
    }

    let current = env!("CARGO_PKG_VERSION");
    match latest_release_tag() {
        Some(tag) => {
            let latest = tag.trim_start_matches('v');
            if latest == current && !force {
                println!("tsmv is already up to date (v{current}).");
                return 0;
            }
            println!("Updating tsmv v{current} -> v{latest} ...");
        }
        None => eprintln!("Could not determine the latest version; attempting update anyway."),
    }

    run_installer()
}

/// Remove the running binary from disk. Returns an exit code.
pub fn self_uninstall(assume_yes: bool) -> i32 {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: cannot locate the tsmv executable: {e}");
            return 1;
        }
    };

    if !assume_yes {
        if !std::io::stdin().is_terminal() {
            eprintln!("Refusing to uninstall without confirmation; re-run with --yes.");
            return 1;
        }
        eprint!("Remove tsmv at {}? [y/N]: ", exe.display());
        let _ = std::io::stderr().flush();
        let mut answer = String::new();
        if std::io::stdin().read_line(&mut answer).is_err()
            || !matches!(answer.trim().to_lowercase().as_str(), "y" | "yes")
        {
            println!("Uninstall cancelled.");
            return 0;
        }
    }

    if cfg!(windows) {
        eprintln!(
            "Automatic uninstall is not supported on Windows; delete {} manually.",
            exe.display()
        );
        return 1;
    }

    match std::fs::remove_file(&exe) {
        Ok(()) => {
            println!("Removed {}.", exe.display());
            0
        }
        Err(e) => {
            eprintln!("error: failed to remove {}: {e}", exe.display());
            1
        }
    }
}

/// Resolve the latest release tag via the GitHub API (best effort; needs curl).
fn latest_release_tag() -> Option<String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let output = Command::new("curl")
        .args(["-fsSL", "-H", "User-Agent: tsmv-self-update", &url])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    json.get("tag_name")?.as_str().map(str::to_string)
}

/// Run the embedded installer, targeting the directory of the current binary so
/// the running executable is replaced in place.
fn run_installer() -> i32 {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: cannot locate the tsmv executable: {e}");
            return 1;
        }
    };
    let install_dir = exe.parent().unwrap_or_else(|| Path::new("."));

    let mut script_path = std::env::temp_dir();
    script_path.push(format!("tsmv-install-{}.sh", std::process::id()));
    if let Err(e) = std::fs::write(&script_path, INSTALL_SCRIPT) {
        eprintln!("error: failed to stage the installer script: {e}");
        return 1;
    }

    let status = Command::new("sh")
        .arg(&script_path)
        .env("TSMV_INSTALL_DIR", install_dir)
        .status();
    let _ = std::fs::remove_file(&script_path);

    match status {
        Ok(s) if s.success() => 0,
        Ok(s) => s.code().unwrap_or(1),
        Err(e) => {
            eprintln!("error: the installer did not complete: {e}");
            1
        }
    }
}
