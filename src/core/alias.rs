//! The tsconfig `paths` alias map, resolved in both directions.
//!
//! Two features need this mapping. Absolute-import conversion turns a file path
//! into an alias specifier; the import updater turns an alias specifier back
//! into the file it points at, so a moved file's importers can be found. Both
//! directions live here so they can never disagree about what an alias means.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::core::import_path::normalize_path;
use crate::core::tsconfig::ResolvedTsConfig;

/// A single `paths` mapping, pre-split so matching is a plain comparison.
#[derive(Debug, Clone)]
struct AliasRule {
    /// The alias pattern from tsconfig, e.g. `@/*`.
    pattern: String,
    /// Literal part of the pattern before `*`; the whole pattern when exact.
    pattern_prefix: String,
    /// Literal part of the pattern after `*`; empty when exact.
    pattern_suffix: String,
    /// Absolute path the target resolves to. For a wildcard rule this is the
    /// directory prefix; for an exact rule it is the file itself.
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

impl PathAliasConfig {
    /// An empty map, used when no tsconfig is available.
    pub fn empty(alias_prefix: &str, base_dir: &Path) -> Self {
        PathAliasConfig {
            prefix: alias_prefix.to_string(),
            base_dir: base_dir.to_path_buf(),
            rules: Vec::new(),
        }
    }

    /// Whether the project declares any usable `paths` mapping.
    pub fn has_rules(&self) -> bool {
        !self.rules.is_empty()
    }

    /// Human-readable `pattern -> target` pairs, for verbose output.
    pub fn describe(&self) -> impl Iterator<Item = (&str, std::path::Display<'_>)> {
        self.rules
            .iter()
            .map(|rule| (rule.pattern.as_str(), rule.target.display()))
    }

    /// Resolve an alias specifier to the absolute path it points at.
    ///
    /// Returns `None` for relative imports and for bare package specifiers such
    /// as `react`, which no alias covers. The returned path carries whatever
    /// extension the mapping implies, which is usually none.
    pub fn resolve(&self, specifier: &str) -> Option<PathBuf> {
        if specifier.starts_with('.') {
            return None;
        }

        // TypeScript picks the mapping with the longest literal prefix.
        let mut best: Option<(usize, PathBuf)> = None;
        for rule in &self.rules {
            let resolved = if rule.wildcard {
                let Some(rest) = specifier.strip_prefix(&rule.pattern_prefix) else {
                    continue;
                };
                let Some(captured) = rest.strip_suffix(&rule.pattern_suffix) else {
                    continue;
                };
                if captured.is_empty() {
                    continue;
                }
                rule.target.join(captured)
            } else if specifier == rule.pattern {
                rule.target.clone()
            } else {
                continue;
            };

            let specificity = rule.pattern_prefix.len();
            let is_better = match &best {
                Some((best_specificity, _)) => specificity > *best_specificity,
                None => true,
            };
            if is_better {
                best = Some((specificity, normalize_path(&resolved)));
            }
        }

        best.map(|(_, path)| path)
    }

    /// Render an absolute path as an alias specifier.
    ///
    /// Returns `None` when no mapping covers the path, since an alias the
    /// compiler cannot resolve is worse than no alias at all.
    pub fn to_alias(&self, path: &Path) -> Option<String> {
        // `rules` is ordered most-specific-first, so the first hit wins.
        for rule in &self.rules {
            if rule.wildcard {
                // Component-wise so `src2/x` never matches a `src/` target.
                let Ok(rest) = path.strip_prefix(&rule.target) else {
                    continue;
                };
                let rest = strip_ts_extension(rest).to_string_lossy().replace('\\', "/");
                if rest.is_empty() {
                    continue;
                }
                return Some(rule.pattern.replace('*', &rest));
            }
            if strip_ts_extension(path) == strip_ts_extension(&rule.target) {
                return Some(rule.pattern.clone());
            }
        }
        None
    }

    /// Render an absolute path as an alias using only `prefix`/`base_dir`.
    ///
    /// This is the fallback for a project that declares no `paths` at all.
    pub fn to_prefixed(&self, path: &Path) -> Option<String> {
        let rest = path.strip_prefix(&self.base_dir).ok()?;
        let rest = strip_ts_extension(rest).to_string_lossy().replace('\\', "/");
        if rest.is_empty() {
            return None;
        }
        Some(format!("{}/{rest}", self.prefix))
    }
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
    let fallback = PathAliasConfig::empty(alias_prefix, &project_root.join("src"));

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
            let (pattern_prefix, pattern_suffix) = match pattern.split_once('*') {
                Some((before, after)) if wildcard => (before.to_string(), after.to_string()),
                _ => (pattern.clone(), String::new()),
            };
            Some(AliasRule {
                pattern: pattern.clone(),
                pattern_prefix,
                pattern_suffix,
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

/// Drop a TypeScript/JavaScript extension so `./a` and `./a.ts` compare equal.
pub fn strip_ts_extension(path: &Path) -> PathBuf {
    match path.extension().and_then(|e| e.to_str()) {
        Some("ts" | "tsx" | "js" | "jsx") => path.with_extension(""),
        _ => path.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build an alias map directly, anchored at `/repo/src`.
    fn map(entries: &[(&str, &str)]) -> PathAliasConfig {
        let paths: HashMap<String, Vec<String>> = entries
            .iter()
            .map(|(k, v)| ((*k).to_string(), vec![(*v).to_string()]))
            .collect();
        PathAliasConfig {
            prefix: "@".into(),
            base_dir: PathBuf::from("/repo/src"),
            rules: build_rules(&paths, Path::new("/repo/src")),
        }
    }

    #[test]
    fn resolves_a_wildcard_alias_to_a_path() {
        let m = map(&[("@/*", "*")]);
        assert_eq!(
            m.resolve("@/components/shell"),
            Some(PathBuf::from("/repo/src/components/shell"))
        );
    }

    #[test]
    fn resolves_an_exact_alias_to_a_path() {
        let m = map(&[("@app", "app.ts")]);
        assert_eq!(m.resolve("@app"), Some(PathBuf::from("/repo/src/app.ts")));
    }

    #[test]
    fn bare_and_relative_specifiers_resolve_to_nothing() {
        let m = map(&[("@/*", "*")]);
        // A package, not a path in this project.
        assert_eq!(m.resolve("react"), None);
        assert_eq!(m.resolve("@scope/pkg"), None);
        // Relative specifiers are the caller's job, not the alias map's.
        assert_eq!(m.resolve("./sibling"), None);
        assert_eq!(m.resolve("../parent"), None);
    }

    #[test]
    fn the_longest_matching_pattern_wins() {
        let m = map(&[("@/*", "*"), ("@/lib/*", "vendor/*")]);
        assert_eq!(
            m.resolve("@/lib/thing"),
            Some(PathBuf::from("/repo/src/vendor/thing")),
            "the more specific @/lib/* mapping should win over @/*"
        );
        assert_eq!(
            m.resolve("@/other/thing"),
            Some(PathBuf::from("/repo/src/other/thing"))
        );
    }

    #[test]
    fn a_bare_pattern_prefix_with_no_captured_segment_does_not_match() {
        let m = map(&[("@/*", "*")]);
        assert_eq!(m.resolve("@/"), None);
    }

    #[test]
    fn resolve_and_to_alias_round_trip() {
        let m = map(&[("@/*", "*")]);
        let path = m.resolve("@/components/shell").unwrap();
        assert_eq!(m.to_alias(&path).as_deref(), Some("@/components/shell"));
    }

    #[test]
    fn to_alias_ignores_a_file_extension() {
        let m = map(&[("@/*", "*")]);
        assert_eq!(
            m.to_alias(Path::new("/repo/src/components/shell.tsx"))
                .as_deref(),
            Some("@/components/shell")
        );
    }

    #[test]
    fn to_alias_declines_paths_outside_every_root() {
        let m = map(&[("@/*", "*")]);
        assert_eq!(m.to_alias(Path::new("/repo/outside/thing.ts")), None);
    }

    #[test]
    fn a_sibling_directory_sharing_a_prefix_is_not_matched() {
        let m = map(&[("@/lib/*", "lib/*")]);
        // "lib2" must not be treated as living under the "lib/" target.
        assert_eq!(m.to_alias(Path::new("/repo/src/lib2/thing.ts")), None);
    }
}
