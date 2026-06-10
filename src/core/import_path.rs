use std::path::{Path, PathBuf};

/// Calculate relative import path from one file's directory to a target file.
/// Returns a path suitable for use in an import statement
/// (no extension, starts with ./ or ../).
pub fn calculate_relative_path(from_dir: &Path, to_file: &Path) -> String {
    let rel = pathdiff::diff_paths(to_file, from_dir).unwrap_or_else(|| to_file.to_path_buf());

    let mut rel_str = rel.to_string_lossy().replace('\\', "/");

    // Strip extension for TypeScript imports
    let ext_to_strip = [".ts", ".tsx", ".js", ".jsx"];
    for ext in &ext_to_strip {
        if rel_str.ends_with(ext) {
            rel_str = rel_str[..rel_str.len() - ext.len()].to_string();
            break;
        }
    }

    // Strip /index if present (TypeScript convention: import './dir' not './dir/index')
    if rel_str.ends_with("/index") {
        rel_str = rel_str[..rel_str.len() - "/index".len()].to_string();
    }

    // Ensure starts with ./ or ../
    if !rel_str.starts_with('.') {
        rel_str = format!("./{rel_str}");
    }

    rel_str
}

/// Resolve a relative import to an absolute file path.
/// Tries various extensions and index files.
pub fn resolve_import_target(import_path: &str, source_file: &Path) -> Option<PathBuf> {
    if !import_path.starts_with('.') {
        return None;
    }

    let source_dir = source_file.parent().unwrap_or(Path::new("."));
    let resolved = source_dir.join(import_path);

    // Normalize (resolve .. and .)
    let resolved = normalize_path(&resolved);

    // If import already has an extension, use as-is
    if let Some(ext) = resolved.extension().and_then(|e| e.to_str()) {
        if matches!(ext, "ts" | "tsx" | "js" | "jsx") {
            return Some(resolved);
        }
    }

    // Try common extensions
    for ext in &["ts", "tsx", "js", "jsx"] {
        let with_ext = PathBuf::from(format!("{}.{ext}", resolved.display()));
        if with_ext.exists() {
            return Some(with_ext);
        }
    }

    // Check for index files
    for ext in &["ts", "tsx"] {
        let index_file = resolved.join(format!("index.{ext}"));
        if index_file.exists() {
            return Some(index_file);
        }
    }

    // Return the resolved path even if it doesn't exist yet
    Some(resolved)
}

/// Normalize a path by resolving . and .. components without requiring filesystem access.
pub(crate) fn normalize_path(path: &Path) -> PathBuf {
    let mut components: Vec<std::path::Component> = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                components.pop();
            }
            std::path::Component::CurDir => {}
            c => components.push(c),
        }
    }
    components.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_relative_path_same_dir() {
        let result = calculate_relative_path(
            Path::new("/src/components"),
            Path::new("/src/components/Button.ts"),
        );
        assert_eq!(result, "./Button");
    }

    #[test]
    fn test_calculate_relative_path_parent_dir() {
        let result = calculate_relative_path(
            Path::new("/src/components"),
            Path::new("/src/utils/helpers.ts"),
        );
        assert_eq!(result, "../utils/helpers");
    }

    #[test]
    fn test_calculate_relative_path_deep() {
        // from: /src/deep/nested/module
        // to:   /src/shallow.ts
        // path: up 3 levels (module -> nested -> deep -> src) then shallow.ts
        let result = calculate_relative_path(
            Path::new("/src/deep/nested/module"),
            Path::new("/src/shallow.ts"),
        );
        assert_eq!(result, "../../../shallow");
    }
}
