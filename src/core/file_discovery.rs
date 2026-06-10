use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::errors::{Result, TsmvError};

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub file_path: PathBuf,
    pub is_directory: bool,
    pub source_dir_root: Option<PathBuf>,
    pub rel_path_from_source_root: Option<PathBuf>,
}

/// Resolve an input path to absolute. If already absolute, pass through.
pub fn resolve_input_path(input: &str, cwd: &Path) -> PathBuf {
    let p = Path::new(input);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        cwd.join(p)
    }
}

/// Collect all TypeScript files from the given sources.
pub fn collect_files_to_process(
    sources: &[String],
    cwd: &Path,
    extensions: &[String],
    verbose: bool,
    recursive: bool,
) -> Result<Vec<FileEntry>> {
    let mut entries: Vec<FileEntry> = Vec::new();

    for src in sources {
        let abs_src = resolve_input_path(src, cwd);

        if abs_src.is_dir() {
            if !recursive {
                return Err(TsmvError::RecursiveRequired(
                    abs_src.display().to_string(),
                ).into());
            }
            if verbose {
                eprintln!("[LOG] Collecting files from directory: {}", abs_src.display());
            }
            // sourceDirRoot = parent of the directory being moved
            let source_dir_root = abs_src.parent().unwrap_or(&abs_src).to_path_buf();
            walk_dir(&abs_src, &source_dir_root, extensions, &mut entries);
        } else if abs_src.is_file() {
            if has_valid_extension(&abs_src, extensions) {
                if verbose {
                    eprintln!("[LOG] Adding file: {}", abs_src.display());
                }
                entries.push(FileEntry {
                    file_path: abs_src,
                    is_directory: false,
                    source_dir_root: None,
                    rel_path_from_source_root: None,
                });
            }
        } else {
            // Try glob pattern
            let pattern = cwd.join(src);
            let pattern_str = pattern.to_string_lossy();

            if verbose {
                eprintln!("[LOG] Attempting glob match for: {src} (cwd: {cwd:?})");
            }

            match glob::glob(&pattern_str) {
                Ok(paths) => {
                    let mut match_count = 0;
                    for entry in paths.flatten() {
                        if entry.is_file() && has_valid_extension(&entry, extensions) {
                            if verbose {
                                eprintln!("[LOG] Adding file from pattern: {}", entry.display());
                            }
                            entries.push(FileEntry {
                                file_path: entry,
                                is_directory: false,
                                source_dir_root: None,
                                rel_path_from_source_root: None,
                            });
                            match_count += 1;
                        }
                    }
                    if verbose && match_count == 0 {
                        eprintln!("[LOG] No files matched pattern: {src}");
                    }
                }
                Err(e) => {
                    if verbose {
                        eprintln!("[LOG] Glob error for {src}: {e}");
                    }
                }
            }
        }
    }

    // Deduplicate by file_path
    let mut seen = HashMap::new();
    entries.retain(|e| seen.insert(e.file_path.clone(), ()).is_none());

    Ok(entries)
}

fn walk_dir(
    dir: &Path,
    source_dir_root: &Path,
    extensions: &[String],
    entries: &mut Vec<FileEntry>,
) {
    let iter = match std::fs::read_dir(dir) {
        Ok(it) => it,
        Err(_) => return,
    };

    for entry in iter.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_dir(&path, source_dir_root, extensions, entries);
        } else if path.is_file() && has_valid_extension(&path, extensions) {
            let relative_path = path
                .strip_prefix(source_dir_root)
                .unwrap_or(&path)
                .to_path_buf();

            entries.push(FileEntry {
                file_path: path,
                is_directory: false,
                source_dir_root: Some(source_dir_root.to_path_buf()),
                rel_path_from_source_root: Some(relative_path),
            });
        }
    }
}

pub fn has_valid_extension(path: &Path, extensions: &[String]) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|ext| {
            let dot_ext = format!(".{ext}");
            extensions.iter().any(|e| {
                e == &dot_ext || e.trim_start_matches('.') == ext
            })
        })
        .unwrap_or(false)
}

/// Extract file paths from FileEntry list (filter out directories).
pub fn extract_file_paths(entries: &[FileEntry]) -> Vec<PathBuf> {
    entries
        .iter()
        .filter(|e| !e.is_directory)
        .map(|e| e.file_path.clone())
        .collect()
}

/// Validate that files were found; error if empty.
pub fn validate_files(files: &[PathBuf]) -> Result<()> {
    if files.is_empty() {
        let msg = "No files matched the provided patterns. Aborting move operation.".to_string();
        eprintln!("{}", colored::Colorize::red(colored::Colorize::bold(msg.as_str())));
        return Err(TsmvError::NoFilesMatched.into());
    }
    Ok(())
}

/// Determine processing mode based on file count.
pub fn determine_processing_mode(file_count: usize) -> ProcessingMode {
    if file_count >= 50 {
        ProcessingMode::Streaming
    } else if file_count >= 35 {
        ProcessingMode::Chunked
    } else if file_count >= 15 {
        ProcessingMode::Surgical
    } else {
        ProcessingMode::Standard
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessingMode {
    Standard,
    Surgical,
    Chunked,
    Streaming,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_input_path_absolute() {
        let result = resolve_input_path("/absolute/path", Path::new("/cwd"));
        assert_eq!(result, PathBuf::from("/absolute/path"));
    }

    #[test]
    fn test_resolve_input_path_relative() {
        let result = resolve_input_path("relative/path", Path::new("/cwd"));
        assert_eq!(result, PathBuf::from("/cwd/relative/path"));
    }

    #[test]
    fn test_has_valid_extension() {
        let exts = vec![".ts".into(), ".tsx".into()];
        assert!(has_valid_extension(Path::new("foo.ts"), &exts));
        assert!(has_valid_extension(Path::new("foo.tsx"), &exts));
        assert!(!has_valid_extension(Path::new("foo.js"), &exts));
        // Double extension bug: .test.ts should still match .ts
        assert!(has_valid_extension(Path::new("foo.test.ts"), &exts));
    }

    #[test]
    fn test_determine_processing_mode() {
        assert_eq!(determine_processing_mode(5), ProcessingMode::Standard);
        assert_eq!(determine_processing_mode(14), ProcessingMode::Standard);
        assert_eq!(determine_processing_mode(15), ProcessingMode::Surgical);
        assert_eq!(determine_processing_mode(34), ProcessingMode::Surgical);
        assert_eq!(determine_processing_mode(35), ProcessingMode::Chunked);
        assert_eq!(determine_processing_mode(49), ProcessingMode::Chunked);
        assert_eq!(determine_processing_mode(50), ProcessingMode::Streaming);
        assert_eq!(determine_processing_mode(100), ProcessingMode::Streaming);
    }
}
