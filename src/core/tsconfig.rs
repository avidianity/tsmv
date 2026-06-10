use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// The raw deserialized form of a tsconfig.json file, before extends resolution.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TsConfigRaw {
    pub extends: Option<String>,
    pub compiler_options: Option<serde_json::Value>,
    pub include: Option<Vec<String>>,
    pub exclude: Option<Vec<String>>,
    pub files: Option<Vec<String>>,
    pub references: Option<serde_json::Value>,
}

/// A resolved tsconfig with extends chain flattened.
/// Only the fields we actually use.
#[derive(Debug, Clone)]
pub struct ResolvedTsConfig {
    pub compiler_options: Option<CompilerOptions>,
    pub include: Option<Vec<String>>,
    pub exclude: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct CompilerOptions {
    pub base_url: Option<String>,
    pub paths: Option<HashMap<String, Vec<String>>>,
}

/// Parse and resolve a tsconfig with its extends chain (max depth 10).
pub fn parse_tsconfig(path: &Path) -> anyhow::Result<ResolvedTsConfig> {
    resolve_tsconfig(path, 0)
}

fn resolve_tsconfig(path: &Path, depth: usize) -> anyhow::Result<ResolvedTsConfig> {
    if depth > 10 {
        return Err(anyhow::anyhow!(
            "tsconfig extends chain too deep at: {}",
            path.display()
        ));
    }

    let content = std::fs::read_to_string(path)?;
    let raw: TsConfigRaw = serde_json::from_str(&content)?;

    let parent = if let Some(ref extends) = raw.extends {
        // Only support relative paths for now (skip npm packages)
        if extends.starts_with('.') {
            let parent_path = path.parent().unwrap_or(Path::new(".")).join(extends);
            // Resolve: if the path doesn't exist directly, try adding .json extension
            let parent_path = if parent_path.exists() {
                parent_path
            } else {
                let with_json = PathBuf::from(format!("{}.json", parent_path.display()));
                if with_json.exists() {
                    with_json
                } else {
                    parent_path
                }
            };

            if parent_path.exists() {
                Some(resolve_tsconfig(&parent_path, depth + 1)?)
            } else {
                None
            }
        } else {
            // npm package extends (e.g. "@tsconfig/node20") — skip, user must configure manually
            None
        }
    } else {
        None
    };

    merge_configs(parent, raw)
}

/// Merge parent and child configs. Child values override parent.
fn merge_configs(parent: Option<ResolvedTsConfig>, child: TsConfigRaw) -> anyhow::Result<ResolvedTsConfig> {
    let compiler_options = merge_compiler_options(
        parent.as_ref().and_then(|p| p.compiler_options.as_ref()),
        &child.compiler_options,
    );

    // include: child overrides parent if child defines it
    let include = if child.include.is_some() {
        child.include
    } else {
        parent.as_ref().and_then(|p| p.include.clone())
    };

    // exclude: child overrides parent if child defines it (TypeScript appends both actually,
    // but for simplicity: child overrides)
    let exclude = if child.exclude.is_some() {
        child.exclude
    } else {
        parent.as_ref().and_then(|p| p.exclude.clone())
    };

    Ok(ResolvedTsConfig {
        compiler_options,
        include,
        exclude,
    })
}

/// Merge compilerOptions: parent provides defaults, child overrides.
fn merge_compiler_options(
    parent: Option<&CompilerOptions>,
    child_value: &Option<serde_json::Value>,
) -> Option<CompilerOptions> {
    match (parent, child_value) {
        (None, None) => None,
        (Some(p), None) => Some(p.clone()),
        (None, Some(v)) => Some(parse_compiler_options(v)),
        (Some(p), Some(v)) => {
            let child = parse_compiler_options(v);
            Some(CompilerOptions {
                base_url: child.base_url.or(p.base_url.clone()),
                // paths: child overrides parent entirely (TypeScript doesn't deep-merge paths)
                paths: child.paths.or(p.paths.clone()),
            })
        }
    }
}

fn parse_compiler_options(value: &serde_json::Value) -> CompilerOptions {
    CompilerOptions {
        base_url: value.get("baseUrl").and_then(|v| v.as_str()).map(String::from),
        paths: value.get("paths")
            .and_then(|v| v.as_object())
            .map(|obj| {
                obj.iter()
                    .map(|(k, v)| {
                        let vals = v.as_array()
                            .map(|a| a.iter().filter_map(|s| s.as_str().map(String::from)).collect())
                            .unwrap_or_default();
                        (k.clone(), vals)
                    })
                    .collect()
            }),
    }
}

/// Find tsconfig.json by walking up from the given directory.
/// Checks for tsconfig.json first, then tsconfig.build.json.
pub fn find_tsconfig(start_dir: Option<&Path>) -> Option<PathBuf> {
    let mut current = start_dir
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    loop {
        let tsconfig = current.join("tsconfig.json");
        if tsconfig.exists() {
            return Some(tsconfig);
        }
        let tsconfig_build = current.join("tsconfig.build.json");
        if tsconfig_build.exists() {
            return Some(tsconfig_build);
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

    None
}

/// Find the most appropriate tsconfig for a set of files.
pub fn find_tsconfig_for_files(file_paths: &[PathBuf]) -> Option<PathBuf> {
    if file_paths.is_empty() {
        return find_tsconfig(None);
    }

    let first_file_dir = file_paths[0].parent().unwrap_or(Path::new("."));
    find_tsconfig(Some(first_file_dir)).or_else(|| find_tsconfig(None))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_find_tsconfig_walks_up() {
        let dir = TempDir::new().unwrap();
        let tsconfig = dir.path().join("tsconfig.json");
        std::fs::write(&tsconfig, r#"{"compilerOptions":{"target":"es2020"}}"#).unwrap();

        let subdir = dir.path().join("src").join("deep");
        std::fs::create_dir_all(&subdir).unwrap();

        let found = find_tsconfig(Some(&subdir));
        assert_eq!(found, Some(tsconfig));
    }

    #[test]
    fn test_find_tsconfig_build_json() {
        let dir = TempDir::new().unwrap();
        let tsconfig_build = dir.path().join("tsconfig.build.json");
        std::fs::write(&tsconfig_build, "{}").unwrap();

        let found = find_tsconfig(Some(dir.path()));
        assert_eq!(found, Some(tsconfig_build));
    }

    #[test]
    fn test_parse_tsconfig_paths() {
        let dir = TempDir::new().unwrap();
        let tsconfig = dir.path().join("tsconfig.json");
        std::fs::write(
            &tsconfig,
            r#"{"compilerOptions":{"baseUrl":"./src","paths":{"@/*":["./src/*"]}}}"#,
        )
        .unwrap();

        let config = parse_tsconfig(&tsconfig).unwrap();
        let opts = config.compiler_options.unwrap();
        assert_eq!(opts.base_url, Some("./src".into()));
        assert!(opts.paths.unwrap().contains_key("@/*"));
    }

    #[test]
    fn test_extends_basic() {
        let dir = TempDir::new().unwrap();

        // Base config with path aliases
        std::fs::write(
            dir.path().join("tsconfig.base.json"),
            r#"{"compilerOptions":{"baseUrl":"./src","paths":{"@/*":["./src/*"]}}}"#,
        )
        .unwrap();

        // Child config extends base, overrides target
        std::fs::write(
            dir.path().join("tsconfig.json"),
            r#"{"extends":"./tsconfig.base.json","compilerOptions":{"target":"es2020","strict":true}}"#,
        )
        .unwrap();

        let config = parse_tsconfig(&dir.path().join("tsconfig.json")).unwrap();
        let opts = config.compiler_options.unwrap();

        // baseUrl inherited from base
        assert_eq!(opts.base_url, Some("./src".into()));
        // paths inherited from base
        assert!(opts.paths.unwrap().contains_key("@/*"));
    }

    #[test]
    fn test_extends_overrides_paths() {
        let dir = TempDir::new().unwrap();

        std::fs::write(
            dir.path().join("tsconfig.base.json"),
            r#"{"compilerOptions":{"baseUrl":"./src","paths":{"@/*":["./src/*"]}}}"#,
        )
        .unwrap();

        // Child overrides paths
        std::fs::write(
            dir.path().join("tsconfig.json"),
            r##"{"extends":"./tsconfig.base.json","compilerOptions":{"paths":{"#/*":["./lib/*"]}}}"##,
        )
        .unwrap();

        let config = parse_tsconfig(&dir.path().join("tsconfig.json")).unwrap();
        let opts = config.compiler_options.unwrap();

        // baseUrl still inherited
        assert_eq!(opts.base_url, Some("./src".into()));
        // paths from child (not from parent)
        let paths = opts.paths.unwrap();
        assert!(!paths.contains_key("@/*"));
        assert!(paths.contains_key("#/*"));
    }

    #[test]
    fn test_extends_chain() {
        let dir = TempDir::new().unwrap();

        std::fs::write(
            dir.path().join("tsconfig.core.json"),
            r#"{"compilerOptions":{"strict":true,"target":"es2020"}}"#,
        )
        .unwrap();

        std::fs::write(
            dir.path().join("tsconfig.base.json"),
            r#"{"extends":"./tsconfig.core.json","compilerOptions":{"baseUrl":"./src","paths":{"@/*":["./src/*"]}}}"#,
        )
        .unwrap();

        std::fs::write(
            dir.path().join("tsconfig.json"),
            r#"{"extends":"./tsconfig.base.json","include":["src/**/*"]}"#,
        )
        .unwrap();

        let config = parse_tsconfig(&dir.path().join("tsconfig.json")).unwrap();
        let opts = config.compiler_options.unwrap();

        // Inherited through chain
        assert_eq!(opts.base_url, Some("./src".into()));
        assert!(opts.paths.unwrap().contains_key("@/*"));
        // include from child
        assert_eq!(config.include, Some(vec!["src/**/*".into()]));
    }

    #[test]
    fn test_extends_without_json_extension() {
        let dir = TempDir::new().unwrap();

        // Create base WITHOUT .json extension (TypeScript allows both)
        std::fs::write(
            dir.path().join("tsconfig.base.json"),
            r#"{"compilerOptions":{"baseUrl":"./src","paths":{"~/*":["./src/*"]}}}"#,
        )
        .unwrap();

        // extends without .json
        std::fs::write(
            dir.path().join("tsconfig.json"),
            r#"{"extends":"./tsconfig.base","compilerOptions":{"target":"es2020"}}"#,
        )
        .unwrap();

        let config = parse_tsconfig(&dir.path().join("tsconfig.json")).unwrap();
        let opts = config.compiler_options.unwrap();

        assert_eq!(opts.base_url, Some("./src".into()));
        assert!(opts.paths.unwrap().contains_key("~/*"));
    }
}
