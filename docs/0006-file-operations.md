# 0006 - File Operations

Ref files: [src/lib/file-operations.service.ts]

## Core orchestrator: `executeFileMove()`

Located at [src/lib/file-operations.service.ts:310-360].

### Transactional pattern

```
1. planFileOperations()     → FileOperation[]  (plan, no execution)
2. executeFileOperations()  → FileOperationResult (executes via ts-morph)
3. project.saveSync()       → SINGLE commit point to disk
4. cleanupEmptyDirectories() → Remove empty dirs after physical move
```

### `planFileOperations()` [line 35-104]

Converts source file paths + destination into a list of operations:

**For files that are part of a directory move** (has `sourceDirRoot`):
- Preserve subdirectory structure: `rel_path = path.relative(sourceDirRoot, sourceFile)`
- Destination: `join(destination, rel_path)`
- Create intermediate directories as `create-dir` operations

**For individual files**:
- If destination has a `.ts/.tsx/.js/.jsx` extension AND single source file → rename (destination = destination as-is)
- Otherwise → destination is a directory, append filename

Returns operations per file:
```rust
enum FileOperation {
    Move { source: PathBuf, dest: PathBuf },
    CreateDir { path: PathBuf },
    RemoveDir { path: PathBuf },
}
```

### `executeFileOperations()` [line 156-222]

Executes operations sequentially:

```
create-dir:  Check !exists, track creation
move:        Check dest doesn't exist (or force), 
             use ts-morph's sourceFile.move(dest),
             track moved file
remove-dir:  Track for cleanup
```

In Rust: `std::fs::rename()` for moves, `std::fs::create_dir_all()` for directories.

### `cleanupEmptyDirectories()` [line 254-275]

After moves, check source directories. If empty (or only contains empty subdirectories), remove them.

At `isDirectoryEmpty()` [line 280-303]:
- Recurse into subdirectories
- If any file found → not empty
- If all contents are themselves empty directories → empty

### `planImportUpdates()` [line 109-150]

Post-move: scans ALL project source files for imports that reference moved files. Creates `ImportUpdate[]` list. See doc 0004 for the path resolution logic.

### `executeImportUpdates()` [line 227-249]

Applies import updates. For each `ImportUpdate`, finds the matching `importDecl` and calls `setModuleSpecifier()`.

## ts-morph's `sourceFile.move()` auto-behavior

Key insight from the original codebase:
- `sourceFile.move(destPath)` in ts-morph does TWO things at [file-operations.service.ts:200]:
  1. Physically moves the file on disk
  2. Updates the virtual project state

The actual **import path updates** in other files are NOT automatic — the code must explicitly call `planImportUpdates()` + `executeImportUpdates()` (or the syntax-aware updater's equivalent) to update imports in OTHER files that reference the moved files.

## Directory structure preservation [line 46-49]

When moving a directory `src/Experience/` to `src/features/`:
- `file = /project/src/Experience/components/Panel.ts`
- `sourceDirRoot = /project/src`  (parent of moved directory)
- `relativePath = Experience/components/Panel.ts`
- `dest = /project/src/features/Experience/components/Panel.ts`

The moved directory name `Experience/` is preserved in the destination path.

## Rust implementation

```rust
// lib/file_operations.rs
use std::path::{Path, PathBuf};
use std::fs;
use anyhow::Result;

struct FileOperationResult {
    moved_files: Vec<PathBuf>,
    updated_imports: usize,
    created_directories: Vec<PathBuf>,
    removed_directories: Vec<PathBuf>,
    errors: Vec<String>,
}

fn execute_file_move(
    files: &[PathBuf],
    destination: &Path,
    force: bool,
    source_dir_root: Option<&Path>,
) -> Result<FileOperationResult> {
    let operations = plan_file_operations(files, destination, source_dir_root);
    let mut result = execute_file_operations(&operations, force)?;

    // Scan project for imports that need updating
    let updates = plan_import_updates(project_files, files, &operations);
    result.updated_imports = execute_import_updates(&updates)?;

    // Cleanup
    let source_dirs = extract_source_directories(files, source_dir_root);
    result.removed_directories = cleanup_empty_directories(&source_dirs);

    Ok(result)
}
```

### Key differences from ts-morph

- No `project.saveSync()` — each file write is its own operation
- No virtual file system — all operations are on real FS
- Import updates must happen AFTER physical file moves (since swc parses from disk)
- Order: move files → re-parse moved files + importing files → update import paths → write
