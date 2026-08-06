use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::core::alias::PathAliasConfig;
use crate::core::import_path::{calculate_relative_path, normalize_path};
use crate::core::import_ast::rewrite_imports;

/// Configuration for the AST-based import updater.
pub struct ImportUpdaterConfig {
    pub verbose: bool,
    /// Extensions (without leading dot) tried when resolving an import to a file.
    pub extensions: Vec<String>,
    /// tsconfig `paths` aliases, so alias specifiers can be followed to the
    /// file they point at and rewritten in the same form.
    pub aliases: PathAliasConfig,
}

/// How a specifier was written, so a rewrite can be emitted the same way.
///
/// A project that imports exclusively through aliases must not have relative
/// paths introduced into it, and vice versa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SpecifierForm {
    Relative,
    Alias,
}

/// Resolve a module specifier to the absolute path it points at.
///
/// Returns `None` for bare package specifiers such as `react`, which refer to
/// no file in this project.
fn resolve_specifier(
    specifier: &str,
    from_dir: &Path,
    aliases: &PathAliasConfig,
) -> Option<(PathBuf, SpecifierForm)> {
    if specifier.starts_with('.') {
        return Some((
            normalize_path(&from_dir.join(specifier)),
            SpecifierForm::Relative,
        ));
    }
    aliases
        .resolve(specifier)
        .map(|path| (path, SpecifierForm::Alias))
}

/// Render `target` as a specifier written the same way as the original.
///
/// An alias import that no mapping covers falls back to a relative path, which
/// is always correct even if it breaks a project's alias-only convention;
/// emitting an unresolvable alias would not be.
fn render_specifier(
    target: &Path,
    form: SpecifierForm,
    from_dir: &Path,
    aliases: &PathAliasConfig,
) -> String {
    if form == SpecifierForm::Alias {
        if let Some(alias) = aliases.to_alias(target) {
            return alias;
        }
    }
    calculate_relative_path(from_dir, target)
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

    let all_files = find_typescript_files(project_root, &config.extensions);

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

    let (content, modified) = rewrite_imports(&original_content, |import_path, _kind| {
        let (resolved, form) = resolve_specifier(import_path, file_dir, &config.aliases)?;
        for (old_path, new_path) in moved_files {
            if resolved_matches_file(&resolved, old_path, &config.extensions) {
                return Some(render_specifier(new_path, form, file_dir, &config.aliases));
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
///
/// A relative import meant something different at the old location and must be
/// recomputed. An alias import points at a fixed path, so it only changes when
/// the file it names was itself moved.
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

    let (content, modified) = rewrite_imports(&file_content, |import_path, _kind| {
        // Resolve the import as it was written, from the ORIGINAL location.
        let (resolved, form) = resolve_specifier(import_path, old_dir, &config.aliases)?;
        let resolved = resolve_to_existing_file(&resolved, &config.extensions);
        let moved_to = moved_files.get(&resolved);

        match form {
            // Location-independent: untouched unless its target moved.
            SpecifierForm::Alias => Some(render_specifier(
                moved_to?,
                form,
                new_dir,
                &config.aliases,
            )),
            // Relative to the file, so recompute whether or not the target moved.
            SpecifierForm::Relative => {
                let target = moved_to.cloned().unwrap_or(resolved);
                Some(render_specifier(&target, form, new_dir, &config.aliases))
            }
        }
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

/// Resolve a path to an existing file by trying the configured extensions and
/// index files. Falls back to `<path>.<first-ext>` (or `.ts`) so a target that was
/// already moved off disk still maps to a key present in the move mapping.
fn resolve_to_existing_file(path: &Path, extensions: &[String]) -> PathBuf {
    if path.exists() {
        return path.to_path_buf();
    }
    for ext in extensions {
        let with_ext = PathBuf::from(format!("{}.{ext}", path.display()));
        if with_ext.exists() {
            return with_ext;
        }
    }
    for ext in extensions {
        let index = path.join(format!("index.{ext}"));
        if index.exists() {
            return index;
        }
    }
    // Fallback: assume first configured extension (or .ts)
    let fallback_ext = extensions.first().map(|s| s.as_str()).unwrap_or("ts");
    PathBuf::from(format!("{}.{fallback_ext}", path.display()))
}

/// Check whether an already-resolved import target refers to `target_file`.
///
/// Import specifiers normally carry no extension, so this also tries the
/// configured extensions and the `dir` -> `dir/index.ext` form.
fn resolved_matches_file(resolved: &Path, target_file: &Path, extensions: &[String]) -> bool {
    if resolved == target_file {
        return true;
    }

    let resolved_str = resolved.to_string_lossy();
    let target_str = target_file.to_string_lossy();

    for ext in extensions {
        if format!("{resolved_str}.{ext}") == target_str {
            return true;
        }
    }

    // import './dir' resolving to './dir/index.{ext}'
    if let Some(file_name) = target_file.file_name() {
        let file_name = file_name.to_string_lossy();
        for ext in extensions {
            if file_name == format!("index.{ext}") {
                let parent = target_file.parent().unwrap_or(Path::new("."));
                if parent == resolved {
                    return true;
                }
            }
        }
    }

    false
}

/// Recursively find all source files matching the configured extensions.
fn find_typescript_files(base_dir: &Path, extensions: &[String]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    scan_directory(base_dir, &mut files, extensions);
    files
}

fn scan_directory(dir: &Path, files: &mut Vec<PathBuf>, extensions: &[String]) {
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
                scan_directory(&path, files, extensions);
            }
        } else if path.is_file() {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if extensions.iter().any(|e| e == ext) {
                    files.push(path);
                }
            }
        }
    }
}
