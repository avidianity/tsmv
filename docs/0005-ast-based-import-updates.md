# 0005 - AST-Based Import Updates

Ref files: [src/lib/syntax-aware-import-updater/syntax-aware-import-updater.service.ts]

## What it does

After files are physically moved, scans ALL project files for imports that reference the moved files and updates the import paths.

## Pipeline (8 steps)

From `updateImportsInMovedFiles()` [line 173-208]:

```
1. refreshProjectFiles()      [line 46-56]
   → sourceFile.refreshFromFileSystemSync() on all files
   → Reconciles ts-morph virtual state with physical FS

2. addMovedFilesToProject()   [line 61-76]
   → project.addSourceFileAtPath(newPath) for each moved file
   → Ensures ts-morph knows about new locations

3. recordAllMoves()            [line 81-89]
   → recordMove(old, new) into global tracker
   → For debugging/history, not strictly needed for correctness

4. detectCircularDependencies() [line 198-201]
   → DFS on adjacency list of moved files
   → Warnings only, does not block

5. updateAllImportReferences() [line 148-167]
   → For each file in project:
     → updateImportsInFile() [line 94-129]
       → For each import declaration in file:
         → If relative import (.):
           → resolveRelativeImport() → absolute path
           → Check if matches any moved file's old path
           → If match: calculateRelativePath(from file's dir, new path)
           → importDecl.setModuleSpecifier(newRelativePath)
         → If non-relative: skip
     → saveSourceFile() → sourceFile.saveSync()

6. Return updated file count
```

## Important: ts-morph specific operations we must replicate

| ts-morph operation | What it does | Rust equivalent with swc |
|-------------------|--------------|--------------------------|
| `project.getSourceFiles()` | List all loaded source files | Track list of parsed files manually |
| `sourceFile.getImportDeclarations()` | Extract import statements from AST | Visit `swc_ecma_ast::ImportDecl` nodes |
| `importDecl.getModuleSpecifierValue()` | Get the import path string | Get `import.src.value` from AST node |
| `importDecl.setModuleSpecifier(newPath)` | Update the import path text | Modify AST node, regenerate source |
| `sourceFile.saveSync()` | Write modified file to disk | Serialize modified AST via `swc_ecma_codegen` |
| `sourceFile.refreshFromFileSystemSync()` | Re-read file from disk | Re-parse file after move |
| `project.addSourceFileAtPath(path)` | Parse and add file to project | Parse + add to tracked list |
| `sourceFile.move(dest)` | Rename file in ts-morph's virtual FS + physically | `std::fs::rename()` + re-parse |

## Updating imports: two strategies

### Strategy A: AST-level modification (recommended)

Using `swc_ecma_visit::VisitMut` trait:

```rust
use swc_ecma_visit::VisitMut;
use swc_ecma_ast::{ModuleItem, ModuleDecl, ImportDecl, Str};

struct ImportUpdater {
    move_mapping: HashMap<PathBuf, PathBuf>,  // old → new
    current_file: PathBuf,
    changes: usize,
}

impl VisitMut for ImportUpdater {
    fn visit_mut_import_decl(&mut self, decl: &mut ImportDecl) {
        let import_path = decl.src.value.to_string();
        if import_path.starts_with('.') {
            let resolved = resolve_import_target(&import_path, &self.current_file);
            if let Some(new_path) = self.move_mapping.get(&resolved) {
                let new_relative = calculate_relative_path(&self.current_file_dir, new_path);
                decl.src = Str {
                    value: new_relative.into(),
                    ..decl.src.clone()
                };
                self.changes += 1;
            }
        }
    }
}
```

### Strategy B: Text-based replacement (simpler fallback)

Regex-based replacement matching the pattern used in [src/lib/simple-import-updater.service.ts:111-154]:
- Read file content as string
- Regex: `import\s+.*?\s+from\s+['"]([^'"]+)['"]`
- For each match, resolve and replace if it points to a moved file
- Write back

## Circular dependency detection [src/lib/syntax-aware-import-updater/circular-dependency-detector.service.ts]

At [line 104-119]:
1. Build adjacency list from moved files' imports
2. DFS with visited + recursion stack sets
3. If node in recursion stack → cycle found
4. Report cycle path (basenames only)

Rust: standard DFS, no special complexity.

## File modifications approach

ts-morph does AST modification in-memory then `saveSync()` writes all at once. In Rust:
- Parse file with `swc_ecma_parser`
- Apply `VisitMut` to modify AST
- Serialize modified AST with `swc_ecma_codegen`
- Write to file with `std::fs::write`

Must ensure original formatting is preserved. swc's codegen with `Config { minify: false }` does reasonable preservation but is not a lossless printer. Consider:
1. Use swc's source map to map AST nodes to source spans
2. Use span-based text replacement instead of full re-emit (harder but preserves formatting)

Alternatively for v1: just re-emit with swc codegen, accept minor formatting differences.
