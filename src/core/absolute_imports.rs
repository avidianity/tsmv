use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::core::import_path::normalize_path;
use crate::core::import_ast::{rewrite_imports, SpecKind};
use crate::core::tsconfig::ResolvedTsConfig;

/// A single `paths` mapping, pre-resolved so matching is a plain path compare.
#[derive(Debug, Clone)]
struct AliasRule {
    /// The alias pattern from tsconfig, e.g. `@/*`.
    pattern: String,
    /// Absolute path the pattern's target resolves to. For a wildcard rule this
    /// is the directory prefix; for an exact rule it is the file itself.
    target: PathBuf,
    wildcard: bool,
}

/// Path aliases resolved against the tsconfig that declared them.
#[derive(Debug, Clone)]
pub struct PathAliasConfig {
    pub prefix: String,
    /// Absolute directory that alias targets resolve against (`baseUrl`).
    pub base_dir: PathBuf,
    rules: Vec<AliasRule>,
}

/// Build alias rules from raw `paths` entries anchored at `base_dir`.
///
/// Rules are ordered most-specific-first so the longest matching target wins,
/// and the order is stable, which a `HashMap` iteration could not guarantee.
fn build_rules(paths: &HashMap<String, Vec<String>>, base_dir: &Path) -> Vec<AliasRule> {
    let mut rules: Vec<AliasRule> = paths
        .iter()
        .filter_map(|(pattern, targets)| {
            let target = targets.first()?;
            let wildcard = pattern.contains('*') && target.contains('*');
            // For a wildcard target the part before `*` is a directory prefix.
            let literal = match target.split_once('*') {
                Some((before, _)) => before,
                None => target.as_str(),
            };
            Some(AliasRule {
                pattern: pattern.clone(),
                target: normalize_path(&base_dir.join(literal)),
                wildcard,
            })
        })
        .collect();

    rules.sort_by(|a, b| {
        b.target
            .components()
            .count()
            .cmp(&a.target.components().count())
            .then_with(|| a.pattern.cmp(&b.pattern))
    });
    rules
}

/// Parse path aliases from a tsconfig.json.
///
/// `project_root` is only the fallback anchor for a project without usable
/// `paths`; a real tsconfig anchors its aliases at its own `baseUrl`.
pub fn parse_path_aliases(
    tsconfig: Option<&ResolvedTsConfig>,
    alias_prefix: &str,
    project_root: &Path,
) -> PathAliasConfig {
    let fallback = PathAliasConfig {
        prefix: alias_prefix.to_string(),
        base_dir: project_root.join("src"),
        rules: Vec::new(),
    };

    let Some(options) = tsconfig.and_then(|c| c.compiler_options.as_ref()) else {
        return fallback;
    };
    let (Some(base_dir), Some(paths)) = (options.alias_base_dir(), options.paths.as_ref()) else {
        return fallback;
    };

    let rules = build_rules(paths, base_dir);
    if rules.is_empty() {
        return fallback;
    }

    PathAliasConfig {
        prefix: alias_prefix.to_string(),
        base_dir: base_dir.to_path_buf(),
        rules,
    }
}

/// Convert a relative import to an absolute (alias) import.
///
/// Returns the import unchanged when it is not relative, or when it points
/// outside every alias root, since inventing an alias for a file the compiler
/// cannot resolve would be worse than leaving a working relative path.
pub fn convert_to_absolute_import(
    relative_import: &str,
    source_file: &Path,
    alias_config: &PathAliasConfig,
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

    // Resolve the import to an absolute, extensionless path.
    let source_dir = source_file.parent().unwrap_or(Path::new("."));
    let resolved = normalize_path(&source_dir.join(relative_import));

    // Match against the tsconfig path aliases, most specific first.
    for rule in &alias_config.rules {
        let absolute = if rule.wildcard {
            // Component-wise so `src2/x` never matches a `src/` target.
            let Ok(rest) = resolved.strip_prefix(&rule.target) else {
                continue;
            };
            let rest = rest.to_string_lossy().replace('\\', "/");
            rule.pattern.replace('*', &rest)
        } else if strip_ts_extension(&resolved) == strip_ts_extension(&rule.target) {
            rule.pattern.clone()
        } else {
            continue;
        };

        if verbose {
            eprintln!("  Converted: {relative_import} \u{2192} {absolute}");
        }
        return absolute;
    }

    // No `paths` mapping matched. When the project declares aliases, inventing
    // one the compiler cannot resolve would break the build, so keep the
    // relative import that already works.
    if !alias_config.rules.is_empty() {
        return relative_import.to_string();
    }

    // No aliases declared: assume the prefix maps to the base directory.
    let Ok(rest) = resolved.strip_prefix(&alias_config.base_dir) else {
        return relative_import.to_string();
    };
    let rest = rest.to_string_lossy().replace('\\', "/");
    let absolute = format!("{}/{}", alias_config.prefix, rest);

    if verbose {
        eprintln!("  Converted (general): {relative_import} \u{2192} {absolute}");
    }
    absolute
}

/// Drop a TypeScript/JavaScript extension so `./a` and `./a.ts` compare equal.
fn strip_ts_extension(path: &Path) -> PathBuf {
    match path.extension().and_then(|e| e.to_str()) {
        Some("ts" | "tsx" | "js" | "jsx") => path.with_extension(""),
        _ => path.to_path_buf(),
    }
}

/// Update all relative imports in a single file to absolute imports.
/// Returns the number of conversions made.
pub fn update_imports_to_absolute(
    file_path: &Path,
    alias_config: &PathAliasConfig,
    verbose: bool,
) -> anyhow::Result<usize> {
    let original = std::fs::read_to_string(file_path)?;

    let (updated, modified) = rewrite_imports(&original, |import_path, kind| {
        // new URL(import.meta.url) and require.context paths are inherently
        // relative — they must never be converted to alias imports.
        if matches!(kind, SpecKind::Url | SpecKind::Context) {
            return None;
        }
        let absolute = convert_to_absolute_import(import_path, file_path, alias_config, verbose);
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
    let alias_config = parse_path_aliases(tsconfig, alias_prefix, project_root);

    if verbose {
        eprintln!("\nConverting relative imports to absolute imports:");
        eprintln!("  Alias prefix: {}", alias_config.prefix);
        eprintln!("  Alias base directory: {}", alias_config.base_dir.display());
        eprintln!("  Path mappings:");
        for rule in &alias_config.rules {
            eprintln!("    {} \u{2192} {}", rule.pattern, rule.target.display());
        }
    }

    let all_files = find_typescript_files(project_root);

    if verbose {
        eprintln!("  Processing {} files...\n", all_files.len());
    }

    let mut total_converted = 0;
    for file in &all_files {
        match update_imports_to_absolute(file, &alias_config, verbose) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::tsconfig::parse_tsconfig;
    use tempfile::TempDir;

    /// Write a tsconfig and resolve the aliases it declares, anchored at its
    /// own directory the way a real run does.
    fn aliases_for(dir: &Path, tsconfig_json: &str, prefix: &str) -> PathAliasConfig {
        let path = dir.join("tsconfig.json");
        std::fs::write(&path, tsconfig_json).unwrap();
        let resolved = parse_tsconfig(&path).unwrap();
        parse_path_aliases(Some(&resolved), prefix, dir)
    }

    /// Convert the import `spec` as it would appear in `src/pages/page.tsx`.
    fn convert(dir: &Path, config: &PathAliasConfig, spec: &str) -> String {
        let source = dir.join("src/pages/page.tsx");
        convert_to_absolute_import(spec, &source, config, false)
    }

    #[test]
    fn base_url_dot_with_src_targets() {
        let dir = TempDir::new().unwrap();
        let cfg = aliases_for(
            dir.path(),
            r#"{"compilerOptions":{"baseUrl":".","paths":{"@/*":["./src/*"]}}}"#,
            "@",
        );
        assert_eq!(convert(dir.path(), &cfg, "../components/panel"), "@/components/panel");
    }

    #[test]
    fn base_url_src_with_bare_wildcard_target() {
        // Regression: targets resolve against baseUrl, so this must not emit
        // "@/src/components/panel", which would resolve to src/src/...
        let dir = TempDir::new().unwrap();
        let cfg = aliases_for(
            dir.path(),
            r#"{"compilerOptions":{"baseUrl":"./src","paths":{"@/*":["*"]}}}"#,
            "@",
        );
        assert_eq!(convert(dir.path(), &cfg, "../components/panel"), "@/components/panel");
    }

    #[test]
    fn paths_without_base_url_anchor_at_the_config() {
        // TypeScript 4.1+ allows `paths` with no `baseUrl`.
        let dir = TempDir::new().unwrap();
        let cfg = aliases_for(
            dir.path(),
            r#"{"compilerOptions":{"paths":{"~/*":["./src/*"]}}}"#,
            "~",
        );
        assert_eq!(convert(dir.path(), &cfg, "../components/panel"), "~/components/panel");
    }

    #[test]
    fn most_specific_alias_wins_and_is_stable() {
        let dir = TempDir::new().unwrap();
        let cfg = aliases_for(
            dir.path(),
            r#"{"compilerOptions":{"baseUrl":"./src","paths":{"@/*":["*"],"@components/*":["components/*"]}}}"#,
            "@",
        );
        // Repeat: a HashMap iteration order would make this flaky.
        for _ in 0..20 {
            assert_eq!(
                convert(dir.path(), &cfg, "../components/panel"),
                "@components/panel"
            );
        }
    }

    #[test]
    fn sibling_directory_sharing_a_name_prefix_does_not_match() {
        // "src2/x" must not be treated as living under the "src/" target.
        let dir = TempDir::new().unwrap();
        let cfg = aliases_for(
            dir.path(),
            r#"{"compilerOptions":{"baseUrl":".","paths":{"@/*":["./src/*"]}}}"#,
            "@",
        );
        let source = dir.path().join("src2/pages/page.tsx");
        let converted = convert_to_absolute_import("../widgets/thing", &source, &cfg, false);
        assert_eq!(converted, "../widgets/thing", "should stay relative");
    }

    #[test]
    fn exact_non_wildcard_mapping_is_honoured() {
        let dir = TempDir::new().unwrap();
        let cfg = aliases_for(
            dir.path(),
            r#"{"compilerOptions":{"baseUrl":".","paths":{"@app":["./src/app.ts"]}}}"#,
            "@",
        );
        assert_eq!(convert(dir.path(), &cfg, "../app"), "@app");
    }

    #[test]
    fn imports_outside_every_alias_root_stay_relative() {
        let dir = TempDir::new().unwrap();
        let cfg = aliases_for(
            dir.path(),
            r#"{"compilerOptions":{"baseUrl":"./src","paths":{"@/*":["*"]}}}"#,
            "@",
        );
        // ../../packages/ui/button sits above the alias base directory.
        assert_eq!(
            convert(dir.path(), &cfg, "../../../packages/ui/button"),
            "../../../packages/ui/button"
        );
    }

    #[test]
    fn non_relative_and_already_aliased_imports_are_untouched() {
        let dir = TempDir::new().unwrap();
        let cfg = aliases_for(
            dir.path(),
            r#"{"compilerOptions":{"baseUrl":"./src","paths":{"@/*":["*"]}}}"#,
            "@",
        );
        assert_eq!(convert(dir.path(), &cfg, "react"), "react");
        assert_eq!(convert(dir.path(), &cfg, "@scope/pkg"), "@scope/pkg");
        assert_eq!(convert(dir.path(), &cfg, "@/already/absolute"), "@/already/absolute");
    }

    #[test]
    fn inherited_base_url_stays_anchored_to_the_config_that_declared_it() {
        // The base config lives at the repo root and declares baseUrl "./src";
        // a nested package extends it. TypeScript keeps that baseUrl pointing
        // at the *root's* src, not the nested package's.
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("tsconfig.base.json"),
            r#"{"compilerOptions":{"baseUrl":"./src","paths":{"@/*":["*"]}}}"#,
        )
        .unwrap();

        let pkg = dir.path().join("packages/web");
        std::fs::create_dir_all(&pkg).unwrap();
        std::fs::write(
            pkg.join("tsconfig.json"),
            r#"{"extends":"../../tsconfig.base.json"}"#,
        )
        .unwrap();

        let resolved = parse_tsconfig(&pkg.join("tsconfig.json")).unwrap();
        let cfg = parse_path_aliases(Some(&resolved), "@", &pkg);
        assert_eq!(cfg.base_dir, dir.path().join("src"));

        let source = dir.path().join("src/pages/page.tsx");
        assert_eq!(
            convert_to_absolute_import("../components/panel", &source, &cfg, false),
            "@/components/panel"
        );
    }
}
