# 0004 - Import Path Resolution

Ref files: [src/lib/import-path.service.ts], [src/lib/syntax-aware-import-updater/import-resolver.service.ts]

## Core function: `resolveImportPath()`

Located at [src/lib/import-path.service.ts:95-139].

### Algorithm

```
Input: ImportPathContext { sourceFile, targetFile, originalImportPath, movedFiles, moveMapping }
Output: ImportPathResult { newImportPath, isInternal, isRelative, requiresUpdate }

1. If import does NOT start with '.' (node_modules, absolute) → return unchanged
2. resolveImportTarget(importPath, sourceFile) → full resolved path to imported file
3. If target is in moveMapping → BOTH source and target are being moved → use target's new location
4. calculateRelativePath(targetFile's new location, import target's resolved path) → new import path
5. requiresUpdate = newPath != originalImportPath
```

### Key detail: internal vs external imports

At [src/lib/import-path.service.ts:122-128]:
- If imported file is ALSO in the move mapping → "internal" — both files move, recalculate relative path from new locations
- If imported file is NOT moving → only the importing file moved → recalculate relative path from importer's new location to static import target

### Path resolution with index files

`resolveImportTarget()` [src/lib/import-path.service.ts:26-49]:
- Only handles `.`-prefixed (relative) imports
- If import path has extension → use as-is
- If no extension → append `.ts`
- Does NOT handle index.ts resolution (handled in syntax-aware version)

Syntax-aware version at [src/lib/syntax-aware-import-updater/import-resolver.service.ts:42-73]:
- `resolveRelativeImport()` tries extensions: `.ts`, `.tsx`, `.js`, `.jsx`
- Falls back to `index.ts` in resolved directory

### Relative path calculation

`calculateRelativePath()` [src/lib/import-path.service.ts:69-88]:
```ts
// fromFile: "/src/new/components/Button.ts"
// toFile:   "/src/utils/helpers.ts"
// result:   "../utils/helpers"   (extension stripped, ./ prefix added)
```
- Strips `.ts/.tsx/.js/.jsx` extension
- Ensures starts with `./` or `../`
- Normalizes `\` to `/` for cross-platform

### Move mapping creation

`createMoveMapping()` [src/lib/import-path.service.ts:161-173]:
- Converts `FileOperation[]` into `Map<old_path, new_path>`

## Rust implementation note

The key challenge: swc does NOT have ts-morph's `sourceFile.move()` that auto-updates imports. We must implement import path resolution ourselves, which this module provides.

### Important edge cases

1. **index.ts resolution**: `import './utils'` resolves to `./utils/index.ts` — handle this
2. **Extension handling**: `import './foo'` → `./foo.ts` or `./foo/index.ts`
3. **Self-imports**: file importing from its own directory, both files move together → must preserve correct relative path
4. **Doubly-nested directory moves**: Moving `src/old/deep/` to `src/new/` — preserve subdirectory structure, calculate relative paths correctly for all nesting levels

### Rust path handling

Use `std::path::Path` and `pathdiff` crate (or manual relative path computation):
```rust
use std::path::{Path, PathBuf};

struct ImportPathContext {
    source_file: PathBuf,       // importing file's original path
    target_file: PathBuf,       // importing file's new path (after move)
    original_import_path: String,
    moved_files: Vec<PathBuf>,
    move_mapping: HashMap<PathBuf, PathBuf>,  // old → new
}

struct ImportPathResult {
    new_import_path: String,
    is_internal: bool,
    is_relative: bool,
    requires_update: bool,
}
```

For `path.relative()` (Node.js built-in) → Rust: `pathdiff::diff_paths(to, from)` returns `Option<PathBuf>`.
