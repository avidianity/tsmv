use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::core::import_path::normalize_path;
use crate::core::import_ast::{rewrite_imports, SpecKind};
use crate::core::tsconfig::ResolvedTsConfig;

/// Configuration extracted from tsconfig.json path aliases.
#[derive(Debug, Clone)]
pub struct PathAliasConfig {
    pub prefix: String,
    pub base_url: String,
    /// Map of alias pattern → target path. e.g. "@/*" → "./src/*"
    pub paths: HashMap<String, String>,
}

impl Default for PathAliasConfig {
    fn default() -> Self {
        let mut paths = HashMap::new();
        paths.insert("@/*".into(), "./src/*".into());
        PathAliasConfig {
            prefix: "@".into(),
            base_url: "./src".into(),
            paths,
        }
    }
}

/// Parse path aliases from a tsconfig.json. Falls back to defaults if missing.
pub fn parse_path_aliases(tsconfig: Option<&ResolvedTsConfig>, alias_prefix: &str) -> PathAliasConfig {
    let default = PathAliasConfig::default();

    let config = match tsconfig.and_then(|c| c.compiler_options.as_ref()) {
        Some(opts) => opts,
        None => return default,
    };

    let base_url = config.base_url.clone().unwrap_or_else(|| default.base_url.clone());
    let raw_paths = match &config.paths {
        Some(p) => p,
        None => return default, // no paths defined = use defaults
    };

    let mut paths = HashMap::new();
    for (alias, targets) in raw_paths {
        if let Some(first) = targets.first() {
            paths.insert(alias.clone(), first.clone());
        }
    }

    if paths.is_empty() {
        return default;
    }

    PathAliasConfig {
        prefix: alias_prefix.to_string(),
        base_url,
        paths,
    }
}

/// Convert a relative import to an absolute import using path aliases.
/// Returns the original path if it's already absolute or not relative.
pub fn convert_to_absolute_import(
    relative_import: &str,
    source_file: &Path,
    alias_config: &PathAliasConfig,
    project_root: &Path,
    verbose: bool,
) -> String {
    // Already absolute (starts with alias prefix)
    if relative_import.starts_with(&alias_config.prefix) {
        return relative_import.to_string();
    }

    // Skip non-relative imports (node_modules, built-ins)
    if !relative_import.starts_with('.') {
        return relative_import.to_string();
    }

    // Resolve the actual file path
    let source_dir = source_file.parent().unwrap_or(Path::new("."));
    let resolved = source_dir.join(relative_import);

    // Normalize path (handle .. and .)
    let resolved = normalize_path(&resolved);

    // Make relative to project root
    let relative_to_project = match pathdiff::diff_paths(&resolved, project_root) {
        Some(p) => p,
        None => return relative_import.to_string(),
    };

    let normalized = relative_to_project.to_string_lossy().replace('\\', "/");

    // Try matching against path alias patterns
    for (alias_pattern, target_pattern) in &alias_config.paths {
        // Strip leading ./ from target, replace * with a match
        let clean_target = target_pattern
            .trim_start_matches("./")
            .replace('*', "");

        if normalized.starts_with(&clean_target) {
            let matched_part = &normalized[clean_target.len()..];
            let absolute = alias_pattern.replace('*', matched_part);

            if verbose {
                eprintln!("  Converted: {relative_import} \u{2192} {absolute}");
            }
            return absolute;
        }
    }

    // Fallback: general prefix-based conversion
    let cleaned = normalized.strip_prefix("src/").unwrap_or(&normalized);
    let absolute = format!("{}/{}", alias_config.prefix, cleaned);

    if verbose {
        eprintln!("  Converted (general): {relative_import} \u{2192} {absolute}");
    }
    absolute
}

/// Update all relative imports in a single file to absolute imports.
/// Returns the number of conversions made.
pub fn update_imports_to_absolute(
    file_path: &Path,
    alias_config: &PathAliasConfig,
    project_root: &Path,
    verbose: bool,
) -> anyhow::Result<usize> {
    let original = std::fs::read_to_string(file_path)?;

    let (updated, modified) = rewrite_imports(&original, |import_path, kind| {
        // new URL(import.meta.url) and require.context paths are inherently
        // relative — they must never be converted to alias imports.
        if matches!(kind, SpecKind::Url | SpecKind::Context) {
            return None;
        }
        let absolute =
            convert_to_absolute_import(import_path, file_path, alias_config, project_root, verbose);
        (absolute != import_path).then_some(absolute)
    });

    if modified {
        std::fs::write(file_path, &updated)?;
        if verbose {
            eprintln!(
                "  Updated: {}",
                file_path.file_name().unwrap_or_default().to_string_lossy()
            );
        }
    }

    Ok(if modified { 1 } else { 0 })
}

/// Convert all relative imports to absolute in an entire project.
pub fn convert_project_to_absolute_imports(
    project_root: &Path,
    tsconfig: Option<&ResolvedTsConfig>,
    alias_prefix: &str,
    verbose: bool,
) -> anyhow::Result<usize> {
    let alias_config = parse_path_aliases(tsconfig, alias_prefix);

    if verbose {
        eprintln!("\nConverting relative imports to absolute imports:");
        eprintln!("  Alias prefix: {}", alias_config.prefix);
        eprintln!("  Base URL: {}", alias_config.base_url);
        eprintln!("  Path mappings:");
        for (alias, target) in &alias_config.paths {
            eprintln!("    {alias} \u{2192} {target}");
        }
    }

    let all_files = find_typescript_files(project_root);

    if verbose {
        eprintln!("  Processing {} files...\n", all_files.len());
    }

    let mut total_converted = 0;
    for file in &all_files {
        match update_imports_to_absolute(file, &alias_config, project_root, verbose) {
            Ok(n) => total_converted += n,
            Err(e) => {
                if verbose {
                    eprintln!("  Warning: failed to update {}: {e}", file.display());
                }
            }
        }
    }

    if verbose {
        eprintln!("\nConverted {total_converted} imports to absolute paths");
    }

    Ok(total_converted)
}

/// Recursively find all TypeScript files.
fn find_typescript_files(base_dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    scan_dir(base_dir, &mut files);
    files
}

fn scan_dir(dir: &Path, files: &mut Vec<PathBuf>) {
    let iter = match std::fs::read_dir(dir) {
        Ok(it) => it,
        Err(_) => return,
    };
    for entry in iter.flatten() {
        let path = entry.path();
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        if path.is_dir() {
            let skip = ["node_modules", "dist", ".git", ".next", "build", "target"];
            if !skip.contains(&name.as_ref()) {
                scan_dir(&path, files);
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
