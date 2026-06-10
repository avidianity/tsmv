# 0010 - Dry-Run Mode

Ref files: [src/lib/dry-run.service.ts], [src/lib/move-files.service.ts:544-561]

## Purpose

When `--dry-run` / `-n` flag is set, show what WOULD happen without making any changes.

## Implementation

At [src/lib/move-files.service.ts:544-561]:
- Runs AFTER file collection (so we know what files exist)
- Runs BEFORE any file operations
- **Returns early** — nothing after this point executes

```ts
if (options.dryRun) {
    const affectedImports = new Map<string, string[]>();
    for (const context of importContexts) {
        const existing = affectedImports.get(context.sourceFile) || [];
        affectedImports.set(context.sourceFile, [...existing, context.originalImportPath]);
    }
    executeDryRun(files, absoluteDestination, affectedImports);
    return { movedFiles: [], updatedImports: 0, ... };  // EMPTY RESULT
}
```

### `executeDryRun()` [dry-run.service.ts:99-111]

```ts
const result = generateDryRunPreview(sourceFiles, destination, affectedImports);
const output = formatDryRunOutput(result);
console.log(output);
return;  // CRITICAL: NO FILE OPERATIONS
```

### `generateDryRunPreview()` [line 28-66]

For each source file:
- Calculate destination path
- Track directories that would be created/removed
- Collect affected imports from the map

Returns `DryRunResult` with:
- `previews[]`: source, destination, operation type, affected imports
- `totalFiles`, `totalImports`
- `wouldCreateDirectories[]`, `wouldRemoveDirectories[]`

### `formatDryRunOutput()` [line 71-93]

Produces formatted output with chalk:
```
DRY RUN MODE: No files will be moved.
The following operations would be performed:

/path/to/source.ts → /path/to/dest.ts
  └─ Would update 3 import(s)

📊 Summary:
  Files to move: 5
  Imports to update: 12
  Directories to create: 3
  Directories to clean: 2
```

## Rust implementation

This is straightforward — same logic without any AST involvement:

```rust
// lib/dry_run.rs

struct DryRunPreview {
    source: PathBuf,
    destination: PathBuf,
    operation: String,  // "move" or "rename"
    affected_imports: Vec<String>,
}

struct DryRunResult {
    previews: Vec<DryRunPreview>,
    total_files: usize,
    total_imports: usize,
    would_create_directories: Vec<PathBuf>,
    would_remove_directories: Vec<PathBuf>,
}

fn execute_dry_run(
    files: &[PathBuf],
    destination: &Path,
    affected_imports: &HashMap<PathBuf, Vec<String>>,
) -> DryRunResult {
    let mut previews = Vec::new();
    let mut would_create = HashSet::new();
    let mut would_remove = HashSet::new();

    for source in files {
        let source_name = source.file_name().unwrap();
        let dest = destination.join(source_name);

        would_create.insert(dest.parent().unwrap().to_path_buf());
        would_remove.insert(source.parent().unwrap().to_path_buf());

        let operation = if source == &dest { "rename" } else { "move" };
        previews.push(DryRunPreview {
            source: source.clone(),
            destination: dest,
            operation: operation.to_string(),
            affected_imports: affected_imports.get(source).cloned().unwrap_or_default(),
        });
    }

    let total_imports = affected_imports.values().map(|v| v.len()).sum();

    DryRunResult {
        previews,
        total_files: files.len(),
        total_imports,
        would_create_directories: would_create.into_iter().collect(),
        would_remove_directories: would_remove.into_iter().collect(),
    }
}
```

### Affected imports detection

For dry-run, we need to estimate which imports would be affected WITHOUT actually parsing the full project. Options:
1. Scan all project `.ts/.tsx` files with regex for imports matching moved file paths (faster)
2. Parse the full project with swc (more accurate)

The original code [move-files.service.ts:545-549] uses the already-parsed project's import contexts. For v1 Rust, regex scan is simpler:

```rust
fn estimate_affected_imports(
    files: &[PathBuf],
    project_root: &Path,
) -> HashMap<PathBuf, Vec<String>> {
    // Walk project files, regex for imports, check if any resolved to moved files
    // Return map: file_path → [import_paths that would change]
}
```
