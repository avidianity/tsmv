use std::path::{Path, PathBuf};

use crate::errors::Result;
use crate::core::absolute_imports::convert_project_to_absolute_imports;
use crate::core::circular_deps::detect_circular_dependencies;
use crate::core::file_discovery::{
    collect_files_to_process, determine_processing_mode, extract_file_paths, validate_files,
    ProcessingMode,
};
use crate::core::file_operations::{
    cleanup_empty_directories, execute_file_operations, find_source_directories,
    plan_file_operations, FileOperation,
};
use crate::core::import_updater::{update_imports_in_project, ImportUpdaterConfig};
use crate::core::tsconfig::{find_tsconfig_for_files, parse_tsconfig};
use crate::options::MoveOptions;

/// Result returned by the move_files orchestrator.
#[derive(Debug, Default)]
pub struct MoveFilesResult {
    pub moved_files: Vec<PathBuf>,
    pub updated_imports: usize,
    pub created_directories: Vec<PathBuf>,
    pub removed_directories: Vec<PathBuf>,
    pub errors: Vec<String>,
}

/// Main orchestrator: collect files, move them, update imports.
pub fn move_files(
    sources: &[String],
    destination: &str,
    options: &MoveOptions,
) -> Result<MoveFilesResult> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    if options.verbose {
        eprintln!("[LOG] Initial CWD: {}", cwd.display());
        eprintln!(
            "[LOG] moveFiles called with sources: {}, destination: {destination}",
            sources.join(", ")
        );
    }

    // Resolve destination
    let absolute_destination = crate::core::file_discovery::resolve_input_path(destination, &cwd);

    if options.verbose {
        eprintln!("[LOG] Absolute destination: {}", absolute_destination.display());
    }

    // Collect files
    let entries = collect_files_to_process(
        sources,
        &cwd,
        &options.extensions,
        options.verbose,
        options.recursive,
    )?;
    let files = extract_file_paths(&entries);

    if options.verbose {
        eprintln!("[LOG] Found {} unique files to move:", files.len());
        for f in &files {
            eprintln!("[LOG]   - {}", f.display());
        }
    }

    validate_files(&files)?;

    // Determine processing mode
    let mode = determine_processing_mode(entries.len());
    if options.verbose {
        eprintln!(
            "[LOG] Using {:?} processing mode for {} files",
            mode,
            entries.len()
        );
    }

    // Extract source directory root (for directory structure preservation)
    let source_dir_root = entries
        .iter()
        .find(|e| e.source_dir_root.is_some())
        .and_then(|e| e.source_dir_root.clone());

    if mode == ProcessingMode::Streaming {
        eprintln!("Streaming mode not yet implemented. Falling back to standard processing.");
    }

    // --- Dry-run mode: preview only ---
    if options.dry_run {
        return execute_dry_run(&files, &absolute_destination, source_dir_root.as_deref());
    }

    // --- Execute the move ---
    let tsconfig_path = options
        .tsconfig
        .clone()
        .or_else(|| find_tsconfig_for_files(&files));

    if options.verbose {
        eprintln!(
            "[LOG] Using tsconfig: {}",
            tsconfig_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "None".to_string())
        );
    }

    // Check for interactive confirmation on overwrites
    if options.interactive {
        let needs_confirm = check_destinations_exist(&files, &absolute_destination, source_dir_root.as_deref());
        if !needs_confirm.is_empty() && !prompt_overwrite_confirm(&needs_confirm) {
            eprintln!("Move cancelled.");
            return Ok(MoveFilesResult::default());
        }
    }

    // Plan and execute file operations
    let operations = plan_file_operations(&files, &absolute_destination, source_dir_root.as_deref());
    let (file_result, move_mapping) =
        execute_file_operations(&operations, options.force);

    if !file_result.errors.is_empty() {
        for err in &file_result.errors {
            eprintln!("[ERROR] {err}");
        }
        if file_result.moved_files.is_empty() {
            return Ok(MoveFilesResult {
                errors: file_result.errors,
                ..Default::default()
            });
        }
    }

    // Update imports in project files after moves
    let mut updated_imports = 0;
    if !move_mapping.is_empty() {
        let project_root = find_project_root(&files);

        updated_imports = update_imports_in_project(
            &move_mapping,
            &project_root,
            &ImportUpdaterConfig {
                verbose: options.verbose,
                extensions: options.resolution_extensions(),
            },
        );
    }

    // Absolute import conversion (Phase 4.2)
    if options.absolute_imports {
        if let Some(ref tsconfig_path) = tsconfig_path {
            let project_root = find_project_root(&files);
            let tsconfig = parse_tsconfig(tsconfig_path).ok();

            match convert_project_to_absolute_imports(
                &project_root,
                tsconfig.as_ref(),
                &options.alias_prefix,
                options.verbose,
            ) {
                Ok(converted) => {
                    if options.verbose {
                        eprintln!("[LOG] Converted {converted} imports to absolute paths");
                    }
                }
                Err(e) => {
                    if options.verbose {
                        eprintln!("[WARN] Failed to convert imports to absolute: {e}");
                    }
                }
            }
        } else if options.verbose {
            eprintln!("[LOG] Skipping absolute imports conversion (no tsconfig found)");
        }
    }

    // Detect circular dependencies among moved files (advisory warning)
    if !move_mapping.is_empty() {
        if let Some(cycle) = detect_circular_dependencies(&move_mapping) {
            eprintln!(
                "{} Circular dependency detected: {}",
                colored::Colorize::yellow("⚠"),
                cycle.join(" → ")
            );
        }
    }

    // Clean up empty source directories
    let source_dirs = find_source_directories(&files, source_dir_root.as_deref());
    let removed_dirs = cleanup_empty_directories(&source_dirs);

    if options.verbose {
        eprintln!(
            "[LOG] Move complete: {} files moved, {} imports updated",
            file_result.moved_files.len(),
            updated_imports
        );
    }

    Ok(MoveFilesResult {
        moved_files: file_result.moved_files,
        updated_imports,
        created_directories: file_result.created_directories,
        removed_directories: removed_dirs,
        errors: file_result.errors,
    })
}

/// Check which destination files already exist (for interactive confirmation).
fn check_destinations_exist(
    sources: &[PathBuf],
    destination: &Path,
    source_dir_root: Option<&Path>,
) -> Vec<PathBuf> {
    let mut existing = Vec::new();
    for source in sources {
        let file_name = source.file_name().unwrap_or_default();
        let dest = if let Some(root) = source_dir_root {
            if let Ok(rel) = source.strip_prefix(root) {
                destination.join(rel)
            } else {
                destination.join(file_name)
            }
        } else {
            destination.join(file_name)
        };
        if dest.exists() {
            existing.push(dest);
        }
    }
    existing
}

/// Prompt user for confirmation before overwriting. Returns true if confirmed.
fn prompt_overwrite_confirm(paths: &[PathBuf]) -> bool {
    use std::io::{self, Write};

    eprintln!("\nThe following destination files already exist:");
    for p in paths {
        eprintln!("  {}", p.display());
    }
    eprint!("\nOverwrite? (y/N): ");

    let _ = io::stderr().flush();
    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_err() {
        return false;
    }

    matches!(input.trim().to_lowercase().as_str(), "y" | "yes")
}

/// Show what would happen without making changes.
fn execute_dry_run(sources: &[PathBuf], destination: &Path, source_dir_root: Option<&Path>) -> Result<MoveFilesResult> {
    use colored::Colorize;

    println!("{}", "DRY RUN MODE: No files will be moved.".blue().bold());
    println!(
        "{}",
        "The following operations would be performed:".blue()
    );
    println!();

    // Reuse the real planner so the preview matches what an actual move would do
    // (directory-structure preservation, single-file renames, etc.).
    let operations = plan_file_operations(sources, destination, source_dir_root);
    let mut count = 0;
    for op in &operations {
        if let FileOperation::Move { source, dest } = op {
            println!("  {} \u{2192} {}", source.display(), dest.display());
            count += 1;
        }
    }

    println!();
    println!("\u{1F4CA} Summary:");
    println!("  Files to move: {count}");

    Ok(MoveFilesResult::default())
}

/// Find the project root by looking for tsconfig.json or package.json,
/// starting from the directory of the first source file.
fn find_project_root(files: &[PathBuf]) -> PathBuf {
    if files.is_empty() {
        return std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    }

    let start_dir = files[0]
        .parent()
        .unwrap_or(Path::new("."))
        .to_path_buf();

    let mut current = start_dir;
    loop {
        if current.join("tsconfig.json").exists() || current.join("package.json").exists() {
            return current;
        }
        if let Some(parent) = current.parent() {
            if parent == current {
                break;
            }
            current = parent.to_path_buf();
        } else {
            break;
        }
    }

    // Fallback to CWD
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}
