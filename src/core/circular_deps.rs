use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::core::import_path::resolve_import_target;
use crate::core::import_regex::collect_import_specifiers;

/// Detect circular dependencies among a set of moved files.
/// Returns the first cycle found (as display names), or None.
pub fn detect_circular_dependencies(moved_files: &HashMap<PathBuf, PathBuf>) -> Option<Vec<String>> {
    // Build adjacency list: new_path → set of moved files it imports
    let mut adjacency: HashMap<PathBuf, HashSet<PathBuf>> = HashMap::new();

    for new_path in moved_files.values() {
        let content = match std::fs::read_to_string(new_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let mut imported_moved: HashSet<PathBuf> = HashSet::new();

        for import_path in collect_import_specifiers(&content) {
            if !import_path.starts_with('.') {
                continue;
            }

            // Resolve the import relative to this file
            if let Some(resolved) = resolve_import_target(&import_path, new_path) {
                // Is this resolved file one of the moved files?
                // Check if it matches a key (old path) or value (new path)
                for (old_path, moved_new_path) in moved_files {
                    if paths_match_simple(&resolved, old_path)
                        || paths_match_simple(&resolved, moved_new_path)
                    {
                        imported_moved.insert(moved_new_path.clone());
                    }
                }
            }
        }

        if !imported_moved.is_empty() {
            adjacency.insert(new_path.clone(), imported_moved);
        }
    }

    // DFS for cycle detection
    let mut visited: HashSet<PathBuf> = HashSet::new();
    let mut in_stack: HashSet<PathBuf> = HashSet::new();

    for new_path in moved_files.values() {
        if !visited.contains(new_path) {
            if let Some(cycle) = dfs_detect_cycle(
                new_path,
                &adjacency,
                &mut visited,
                &mut in_stack,
                Vec::new(),
            ) {
                return Some(
                    cycle
                        .iter()
                        .map(|p| p.file_name().unwrap_or_default().to_string_lossy().to_string())
                        .collect(),
                );
            }
        }
    }

    None
}

fn dfs_detect_cycle(
    node: &PathBuf,
    adjacency: &HashMap<PathBuf, HashSet<PathBuf>>,
    visited: &mut HashSet<PathBuf>,
    in_stack: &mut HashSet<PathBuf>,
    path: Vec<PathBuf>,
) -> Option<Vec<PathBuf>> {
    if in_stack.contains(node) {
        let mut cycle = path;
        cycle.push(node.clone());
        return Some(cycle);
    }
    if visited.contains(node) {
        return None;
    }

    visited.insert(node.clone());
    in_stack.insert(node.clone());
    let mut path = path;
    path.push(node.clone());

    if let Some(deps) = adjacency.get(node) {
        for dep in deps {
            if let Some(cycle) = dfs_detect_cycle(dep, adjacency, visited, in_stack, path.clone()) {
                return Some(cycle);
            }
        }
    }

    in_stack.remove(node);
    None
}

/// Simple path comparison: resolve .. and . then compare.
fn paths_match_simple(a: &Path, b: &Path) -> bool {
    fn normalize(p: &Path) -> PathBuf {
        let mut components = Vec::new();
        for c in p.components() {
            match c {
                std::path::Component::ParentDir => {
                    components.pop();
                }
                std::path::Component::CurDir => {}
                _ => components.push(c),
            }
        }
        components.into_iter().collect()
    }

    normalize(a) == normalize(b)
}
