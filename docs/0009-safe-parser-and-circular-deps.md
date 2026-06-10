# 0009 - Safe Parser and Circular Dependencies

Ref files: [src/lib/safe-parser.service.ts], [src/lib/syntax-aware-import-updater/circular-dependency-detector.service.ts]

## Safe Parser

Purpose: Extract imports and exports from TypeScript files without stack overflows on deeply nested code (complex JSX, large objects).

### Approach: Iterative BFS (not recursive DFS)

At [src/lib/safe-parser.service.ts:94-168]:

```
nodeQueue = [{ node: sourceFile, depth: 0 }]

while nodeQueue not empty:
  pop front
  check depth > maxDepth (100) → skip
  check timeout (5000ms) → skip, set timedOut
  check too many skipped nodes (>50) → skip
  check isComplexNode (nested {} >10, JSX attrs >20, text >50000 chars) → skip
  
  if ImportDeclaration → collect
  if ExportDeclaration → collect
  
  push all children to queue with depth+1
```

### Fallback parser [line 173-199]

If stack overflow or other error, fall back to:
- Only inspect `sourceFile.getStatements()` (top-level only)
- Extract top-level import/export declarations

### Rust implementation

swc's AST is visited via `swc_ecma_visit::Visit` trait. The visitor pattern in swc is inherently depth-first (recursive), but Rust's stack is larger than JavaScript's and the visitor is iterative at the swc level. We likely don't need the custom BFS — swc's `visit_module()` handles this.

However, for very large files, we can still implement timeout + skip mechanism:

```rust
use std::time::{Duration, Instant};

struct SafeParseResult {
    imports: Vec<ImportDecl>,
    exports: Vec<ExportDecl>,
    skipped_nodes: usize,
    max_depth_reached: usize,
    timed_out: bool,
}

struct SafeImportExtractor {
    imports: Vec<ImportDecl>,
    exports: Vec<ExportDecl>,
    start_time: Instant,
    max_depth: usize,
    timeout: Duration,
    skipped_nodes: usize,
    current_depth: usize,
}

impl Visit for SafeImportExtractor {
    fn visit_import_decl(&mut self, decl: &ImportDecl) {
        if self.should_skip() { return; }
        self.imports.push(decl.clone());
    }

    fn visit_export_decl(&mut self, decl: &ExportDecl) {
        if self.should_skip() { return; }
        self.exports.push(decl.clone());
    }

    fn should_skip(&self) -> bool {
        if self.current_depth > self.max_depth { return true; }
        if self.start_time.elapsed() > self.timeout { return true; }
        false
    }
}
```

The isComplexNode check [line 67-88] (object depth, JSX attrs, text length) is TypeScript-specific concerns. swc's parser handles these natively — no need for equivalent in Rust.

## Circular Dependency Detection

At [circular-dependency-detector.service.ts:104-119]:

### Algorithm

1. **Build adjacency list** [line 16-44]:
   - For each moved file (new location):
   - Read its imports using ts-morph's `getImportDeclarations()`
   - Resolve each relative import to absolute path
   - If resolved path is another moved file → add edge

2. **DFS cycle detection** [line 49-92]:
   - Standard visited + recursion stack approach
   - For each unmoved file:
     - Enter DFS: mark visited, add to recursion stack
     - For each dependency:
       - If in recursion stack → CYCLE FOUND
       - If not visited → recurse
     - Remove from recursion stack

3. **Display** [line 97-99]: Convert file paths to basenames for warning message

### Rust implementation

```rust
use std::collections::{HashMap, HashSet};

fn detect_circular_dependencies(
    move_map: &HashMap<PathBuf, PathBuf>,  // old → new
    project_files: &[PathBuf],
) -> Option<Vec<PathBuf>> {
    // Build adjacency list: file → set of files it imports
    let mut adj: HashMap<PathBuf, HashSet<PathBuf>> = HashMap::new();
    for new_path in move_map.values() {
        let content = std::fs::read_to_string(new_path).unwrap_or_default();
        let imports = extract_relative_imports(&content);
        let mut deps = HashSet::new();
        for import_path in imports {
            let resolved = resolve_import(&import_path, new_path);
            if move_map.contains_key(&resolved) || move_map.values().any(|v| v == &resolved) {
                deps.insert(resolved);
            }
        }
        adj.insert(new_path.clone(), deps);
    }

    // DFS with recursion stack
    let mut visited: HashSet<&PathBuf> = HashSet::new();
    let mut in_stack: HashSet<&PathBuf> = HashSet::new();

    fn dfs(
        node: &PathBuf,
        adj: &HashMap<PathBuf, HashSet<PathBuf>>,
        visited: &mut HashSet<&PathBuf>,
        in_stack: &mut HashSet<&PathBuf>,
        path: &mut Vec<PathBuf>,
    ) -> Option<Vec<PathBuf>> {
        if in_stack.contains(node) {
            path.push(node.clone());
            return Some(path.clone());
        }
        if visited.contains(node) {
            return None;
        }
        visited.insert(node);
        in_stack.insert(node);
        path.push(node.clone());

        if let Some(deps) = adj.get(node) {
            for dep in deps {
                if let Some(cycle) = dfs(dep, adj, visited, in_stack, &mut path.clone()) {
                    return Some(cycle);
                }
            }
        }

        in_stack.remove(node);
        None
    }

    for new_path in move_map.values() {
        if !visited.contains(new_path) {
            if let Some(cycle) = dfs(new_path, &adj, &mut visited, &mut in_stack, &mut vec![]) {
                return Some(cycle);
            }
        }
    }

    None
}
```

Note: circular dependency detection is **advisory only** — produces a warning, does not block the move operation [line 201-202].
