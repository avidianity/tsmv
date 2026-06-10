use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::core::import_path::{calculate_relative_path, normalize_path};
use crate::core::import_ast::rewrite_imports;

/// Configuration for the AST-based import updater.
pub struct ImportUpdaterConfig {
    pub verbose: bool,
}

/// Update imports in all TypeScript files within a project directory
/// after files have been moved. Returns the number of distinct files modified.
pub fn update_imports_in_project(
    moved_files: &HashMap<PathBuf, PathBuf>, // old -> new
    project_root: &Path,
    config: &ImportUpdaterConfig,
) -> usize {
    if config.verbose {
        eprintln!("Starting simple import updates...");
        eprintln!("Processing {} moved files", moved_files.len());
        eprintln!("Project root: {}", project_root.display());
    }

    let all_files = find_typescript_files(project_root);

    if config.verbose {
        eprintln!("Found {} TypeScript files to check", all_files.len());
    }

    // Track distinct files we modify so a file touched by both passes is counted once.
    let mut modified: HashSet<PathBuf> = HashSet::new();

    // Pass 1: update imports in all files that reference moved files (old→new paths).
    for file in &all_files {
        match update_imports_in_file(file, moved_files, config) {
            Ok(true) => {
                modified.insert(file.clone());
            }
            Err(e) if config.verbose => {
                eprintln!("Error processing {}: {e}", file.display());
            }
            _ => {}
        }
    }

    // Pass 2: recalculate the moved files' own imports relative to their new locations.
    // A moved file's import of a non-moved sibling (e.g. ./sibling) is now at a
    // different path and must be recomputed.
    for (old_path, new_path) in moved_files {
        if let Ok(true) = recalculate_own_imports(old_path, new_path, moved_files, config) {
            modified.insert(new_path.clone());
        }
    }

    if config.verbose {
        eprintln!("Updated imports in {} files", modified.len());
    }

    modified.len()
}

/// Update imports in a single file. Returns true if the file was modified.
fn update_imports_in_file(
    file_path: &Path,
    moved_files: &HashMap<PathBuf, PathBuf>,
    config: &ImportUpdaterConfig,
) -> anyhow::Result<bool> {
    let original_content = std::fs::read_to_string(file_path)?;
    let file_dir = file_path.parent().unwrap_or(Path::new("."));

    let (content, modified) = rewrite_imports(&original_content, |import_path| {
        if !import_path.starts_with('.') {
            return None;
        }
        for (old_path, new_path) in moved_files {
            if does_import_resolve_to_file(import_path, file_path, old_path) {
                return Some(calculate_relative_path(file_dir, new_path));
            }
        }
        None
    });

    if modified {
        std::fs::write(file_path, &content)?;
        if config.verbose {
            eprintln!(
                "  Updated imports in: {}",
                file_path.file_name().unwrap_or_default().to_string_lossy()
            );
        }
    }

    Ok(modified)
}

/// Pass 2: recalculate a moved file's own imports from its new location.
/// Handles the case where the moved file imports a sibling that was NOT moved.
fn recalculate_own_imports(
    old_path: &Path,
    new_path: &Path,
    moved_files: &HashMap<PathBuf, PathBuf>,
    config: &ImportUpdaterConfig,
) -> anyhow::Result<bool> {
    if !new_path.exists() {
        return Ok(false);
    }

    let file_content = std::fs::read_to_string(new_path)?;
    let old_dir = old_path.parent().unwrap_or(Path::new("."));
    let new_dir = new_path.parent().unwrap_or(Path::new("."));

    let (content, modified) = rewrite_imports(&file_content, |import_path| {
        if !import_path.starts_with('.') {
            return None;
        }

        // Resolve the import as it was written, relative to the OLD location.
        let target = normalize_path(&old_dir.join(import_path));
        let resolved_target = resolve_to_existing_file(&target);

        // If that target was itself moved, follow it to its new location.
        let final_target = moved_files
            .get(&resolved_target)
            .cloned()
            .unwrap_or(resolved_target);

        // Recompute the path from the file's NEW location.
        Some(calculate_relative_path(new_dir, &final_target))
    });

    if modified {
        std::fs::write(new_path, &content)?;
        if config.verbose {
            eprintln!(
                "  Recalculated own imports: {}",
                new_path.file_name().unwrap_or_default().to_string_lossy()
            );
        }
    }

    Ok(modified)
}

/// Resolve a path to an existing file by trying common extensions and index files.
/// Falls back to `<path>.ts` so a target that was already moved off disk still maps
/// to a key present in the move mapping.
fn resolve_to_existing_file(path: &Path) -> PathBuf {
    if path.exists() {
        return path.to_path_buf();
    }
    for ext in &["ts", "tsx", "js", "jsx"] {
        let with_ext = PathBuf::from(format!("{}.{ext}", path.display()));
        if with_ext.exists() {
            return with_ext;
        }
    }
    let index = path.join("index.ts");
    if index.exists() {
        return index;
    }
    let index_tsx = path.join("index.tsx");
    if index_tsx.exists() {
        return index_tsx;
    }
    // Fallback: assume .ts
    PathBuf::from(format!("{}.ts", path.display()))
}

/// Check if an import path resolves to a specific file (path math only, no disk check).
fn does_import_resolve_to_file(import_path: &str, from_file: &Path, target_file: &Path) -> bool {
    if !import_path.starts_with('.') {
        return false;
    }

    let source_dir = from_file.parent().unwrap_or(Path::new("."));
    let resolved = normalize_path(&source_dir.join(import_path));

    let resolved_str = resolved.to_string_lossy();
    let target_str = target_file.to_string_lossy();

    // Direct match
    if resolved_str == target_str {
        return true;
    }

    // Try with common TypeScript extensions
    for ext in &["ts", "tsx", "js", "jsx"] {
        let with_ext = format!("{resolved_str}.{ext}");
        if with_ext == target_str {
            return true;
        }
    }

    // Check for index file resolution: import './dir' resolving to './dir/index.ts'
    if let Some(file_name) = target_file.file_name() {
        let file_name = file_name.to_string_lossy();
        if file_name == "index.ts" || file_name == "index.tsx" {
            let parent = target_file.parent().unwrap_or(Path::new("."));
            if parent.to_string_lossy() == resolved_str {
                return true;
            }
        }
    }

    false
}

/// Recursively find all TypeScript/JavaScript files in a directory.
fn find_typescript_files(base_dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    scan_directory(base_dir, &mut files);
    files
}

fn scan_directory(dir: &Path, files: &mut Vec<PathBuf>) {
    let iter = match std::fs::read_dir(dir) {
        Ok(it) => it,
        Err(_) => return,
    };

    for entry in iter.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let dir_name = path.file_name().unwrap_or_default().to_string_lossy();
            // Skip common non-source directories
            let skip = ["node_modules", "dist", ".git", ".next", "build", "target"];
            if !skip.contains(&dir_name.as_ref()) {
                scan_directory(&path, files);
            }
        } else if path.is_file() {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if matches!(ext, "ts" | "tsx" | "js" | "jsx") {
                    files.push(path);
                }
            }
        }
    }
}
