use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub enum FileOperation {
    CreateDir { path: PathBuf },
    Move { source: PathBuf, dest: PathBuf },
}

#[derive(Debug, Default)]
pub struct FileOperationResult {
    pub moved_files: Vec<PathBuf>,
    pub updated_imports: usize,
    pub created_directories: Vec<PathBuf>,
    pub removed_directories: Vec<PathBuf>,
    pub errors: Vec<String>,
}

/// Plan file operations: what to create, what to move.
pub fn plan_file_operations(
    source_files: &[PathBuf],
    destination: &Path,
    source_dir_root: Option<&Path>,
) -> Vec<FileOperation> {
    let mut operations = Vec::new();

    for source_file in source_files {
        let file_name = source_file.file_name().unwrap_or_default();

        let dest_path = if let Some(root) = source_dir_root {
            // Preserve directory structure when moving from a directory
            let relative = match source_file.strip_prefix(root) {
                Ok(rel) => rel.to_path_buf(),
                Err(_) => {
                    // File is directly under source_dir_root? No, it's inside the moved dir.
                    // Try stripping the parent of the moved directory
                    if let Some(parent) = root.parent() {
                        source_file
                            .strip_prefix(parent)
                            .unwrap_or_else(|_| Path::new(file_name))
                            .to_path_buf()
                    } else {
                        PathBuf::from(file_name)
                    }
                }
            };
            let dest = destination.join(&relative);

            // Ensure destination subdirectory exists
            if let Some(parent) = dest.parent() {
                operations.push(FileOperation::CreateDir {
                    path: parent.to_path_buf(),
                });
            }

            dest
        } else if destination_is_file_rename(destination, source_files.len()) {
            // Single file rename: destination IS the new file path
            destination.to_path_buf()
        } else {
            // Multiple files or destination is directory: append filename
            let dest = destination.join(file_name);

            if let Some(parent) = dest.parent() {
                operations.push(FileOperation::CreateDir {
                    path: parent.to_path_buf(),
                });
            }

            dest
        };

        operations.push(FileOperation::Move {
            source: source_file.clone(),
            dest: dest_path,
        });
    }

    operations
}

/// Check if destination looks like a file rename (has a TS extension and only one source).
fn destination_is_file_rename(dest: &Path, source_count: usize) -> bool {
    if source_count != 1 {
        return false;
    }
    match dest.extension().and_then(|e| e.to_str()) {
        Some(ext) => matches!(ext, "ts" | "tsx" | "js" | "jsx"),
        None => false,
    }
}

/// Execute file operations (actual filesystem changes).
/// Returns a mapping of old_path -> new_path for import updates.
pub fn execute_file_operations(
    operations: &[FileOperation],
    force: bool,
) -> (FileOperationResult, HashMap<PathBuf, PathBuf>) {
    let mut result = FileOperationResult::default();
    let mut move_mapping: HashMap<PathBuf, PathBuf> = HashMap::new();

    // First pass: create directories
    for op in operations {
        if let FileOperation::CreateDir { path } = op {
            if !path.exists() {
                if let Err(e) = std::fs::create_dir_all(path) {
                    result.errors.push(format!(
                        "Failed to create directory {}: {e}",
                        path.display()
                    ));
                } else {
                    result.created_directories.push(path.clone());
                }
            }
        }
    }

    // Second pass: move files
    for op in operations {
        if let FileOperation::Move { source, dest } = op {
            if !source.exists() {
                result.errors.push(format!(
                    "Source file not found: {}",
                    source.display()
                ));
                continue;
            }

            if dest.exists() && !force {
                result.errors.push(format!(
                    "Destination already exists: {} (use --force to overwrite)",
                    dest.display()
                ));
                continue;
            }

            if dest.exists() && force {
                if let Err(e) = std::fs::remove_file(dest) {
                    result.errors.push(format!(
                        "Failed to remove existing destination {}: {e}",
                        dest.display()
                    ));
                    continue;
                }
            }

            match std::fs::rename(source, dest) {
                Ok(()) => {
                    result.moved_files.push(dest.clone());
                    move_mapping.insert(source.clone(), dest.clone());
                }
                Err(e) => {
                    result.errors.push(format!(
                        "Failed to move {} -> {}: {e}",
                        source.display(),
                        dest.display()
                    ));
                }
            }
        }
    }

    (result, move_mapping)
}

/// Clean up empty directories after files have been moved out.
pub fn cleanup_empty_directories(source_directories: &[PathBuf]) -> Vec<PathBuf> {
    let mut removed = Vec::new();

    for dir in source_directories {
        // Ignore cleanup errors — leaving a stray directory is harmless.
        if is_directory_empty(dir) && std::fs::remove_dir_all(dir).is_ok() {
            removed.push(dir.clone());
        }
    }

    removed
}

/// Recursively check if a directory is empty or only contains empty subdirectories.
fn is_directory_empty(dir: &Path) -> bool {
    if !dir.exists() {
        return false;
    }

    let iter = match std::fs::read_dir(dir) {
        Ok(it) => it,
        Err(_) => return false,
    };

    for entry in iter.flatten() {
        let path = entry.path();
        if path.is_file() {
            return false;
        }
        if path.is_dir() && !is_directory_empty(&path) {
            return false;
        }
    }

    true
}

/// Find source directories to clean up after a move.
pub fn find_source_directories(
    source_files: &[PathBuf],
    source_dir_root: Option<&Path>,
) -> Vec<PathBuf> {
    if let Some(root) = source_dir_root {
        // Find unique immediate subdirectories of sourceDirRoot that contained moved files
        let mut dirs: Vec<PathBuf> = source_files
            .iter()
            .filter_map(|f| {
                let rel = f.strip_prefix(root).ok()?;
                let first_component = rel.components().next()?;
                Some(root.join(first_component))
            })
            .collect();
        dirs.sort();
        dirs.dedup();
        dirs
    } else {
        // Individual files: clean their immediate parent directories
        let mut dirs: Vec<PathBuf> = source_files
            .iter()
            .filter_map(|f| f.parent().map(|p| p.to_path_buf()))
            .collect();
        dirs.sort();
        dirs.dedup();
        dirs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_plan_file_operations_single_file() {
        let ops = plan_file_operations(
            &[PathBuf::from("/src/Button.ts")],
            Path::new("/src/components"),
            None,
        );
        assert_eq!(ops.len(), 2); // CreateDir + Move
        match &ops[0] {
            FileOperation::CreateDir { path } => {
                assert_eq!(path, Path::new("/src/components"));
            }
            _ => panic!("Expected CreateDir"),
        }
        match &ops[1] {
            FileOperation::Move { source, dest } => {
                assert_eq!(source, Path::new("/src/Button.ts"));
                assert_eq!(dest, Path::new("/src/components/Button.ts"));
            }
            _ => panic!("Expected Move"),
        }
    }

    #[test]
    fn test_plan_file_operations_file_rename() {
        let ops = plan_file_operations(
            &[PathBuf::from("/src/Button.ts")],
            Path::new("/src/NewButton.ts"),
            None,
        );
        // File rename: only the Move operation, no CreateDir needed
        // (parent directory /src already exists)
        assert_eq!(ops.len(), 1);
        match &ops[0] {
            FileOperation::Move { source, dest } => {
                assert_eq!(source, Path::new("/src/Button.ts"));
                assert_eq!(dest, Path::new("/src/NewButton.ts"));
            }
            _ => panic!("Expected Move"),
        }
    }

    #[test]
    fn test_execute_file_operations() {
        let dir = TempDir::new().unwrap();
        let src = dir.path().join("source.ts");
        let dest_dir = dir.path().join("dest");

        std::fs::write(&src, "export const x = 1;").unwrap();

        let ops = plan_file_operations(std::slice::from_ref(&src), &dest_dir, None);
        let (result, mapping) = execute_file_operations(&ops, false);

        assert!(result.errors.is_empty());
        assert_eq!(result.moved_files.len(), 1);
        assert!(dest_dir.join("source.ts").exists());
        assert!(!src.exists());
        assert!(mapping.contains_key(&src));
    }

    #[test]
    fn test_execute_file_operations_force_overwrite() {
        let dir = TempDir::new().unwrap();
        let src = dir.path().join("source.ts");
        let dest = dir.path().join("dest").join("source.ts");

        std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
        std::fs::write(&src, "export const x = 1;").unwrap();
        std::fs::write(&dest, "export const y = 2;").unwrap();

        let ops = plan_file_operations(std::slice::from_ref(&src), &dir.path().join("dest"), None);
        let (result, _) = execute_file_operations(&ops, true);

        assert!(result.errors.is_empty());
        assert!(dest.exists());
        assert!(!src.exists());
    }

    #[test]
    fn test_is_directory_empty() {
        let dir = TempDir::new().unwrap();
        // Create a clean empty subdirectory to test
        let empty_sub = dir.path().join("empty");
        std::fs::create_dir(&empty_sub).unwrap();
        assert!(is_directory_empty(&empty_sub));

        // Create a subdirectory with a file - should not be empty
        let not_empty = dir.path().join("not-empty");
        std::fs::create_dir(&not_empty).unwrap();
        std::fs::write(not_empty.join("file.txt"), "content").unwrap();
        assert!(!is_directory_empty(&not_empty));
    }
}
