# 0003 - File Discovery and Validation

Ref files: [src/lib/move-files.service.ts:50-146], [src/commands/utils.ts]

## Core function: `collectFilesToProcess()`

Located at [src/lib/move-files.service.ts:58-146].

Inputs: `sources: string[]`, `cwd: string`, `extensions: string[]`, `verbose: bool`
Output: `FileEntry[]` (deduplicated by `filePath`)

### Algorithm

For each source path:
1. **Resolve** to absolute: `resolveInputPath(src, cwd)` [line 67] — uses `path.resolve(cwd, src)` if relative, passes through if absolute
2. **Check existence** with `fs.existsSync()`

Three cases:

#### A. Source is a directory [line 69-98]
- Walk directory recursively using `fs.readdirSync()` with `withFileTypes: true`
- Filter by extensions (`.ts`, `.tsx` by default)
- Record `sourceDirRoot = path.dirname(absSrc)` — preserves directory structure when moving
- Each file gets `relPathFromSourceRoot` for reconstructing destination path
- Example: moving `/src/components/` → `sourceDirRoot = /src`, file `Button.tsx` gets `relPathFromSourceRoot = "components/Button.tsx`

#### B. Source is a single file [line 99-109]
- Check extension, push FileEntry with `isDirectory: false`

#### C. Source is a glob pattern [line 110-139]
- Uses `fast-glob` (`fg.sync()`) with `dot: true, onlyFiles: true`
- Matches resolved against cwd
- Filters by extensions

### Deduplication [line 142-145]
```ts
Array.from(new Map(filesToProcess.map(item => [item.filePath, item])).values())
```

### FileEntry structure [src/lib/move-files.service.ts:32-37]
```rust
struct FileEntry {
    file_path: PathBuf,
    is_directory: bool,
    source_dir_root: Option<PathBuf>,
    rel_path_from_source_root: Option<PathBuf>,
}
```

## Helper functions (same file)

### `resolveInputPath()` [line 50-53]
```rust
fn resolve_input_path(input: &str, cwd: &Path) -> PathBuf {
    let p = Path::new(input);
    if p.is_absolute() { p.to_path_buf() } else { cwd.join(p) }
}
```

### `extractFilePaths()` [line 151-155]
Filters FileEntry to only non-directory, extracts `file_path`.

### `validateFiles()` [line 160-166]
Empties check — throws if no files found.

## tsconfig Discovery

Ref: [src/commands/utils.ts:11-140]

### `findTsConfig()` — Walk-up algorithm [line 11-33]

Starts from given directory, goes up to filesystem root, looking for `tsconfig.json` or `tsconfig.build.json`:
```
1. Check <dir>/tsconfig.json
2. Check <dir>/tsconfig.build.json
3. If not found, go to parent directory
4. Repeat until root
5. Return None if nothing found
```

### `findAllTsConfigs()` — Monorepo support [line 41-72]

Recursively finds ALL tsconfig files in subtree. Skips `node_modules` and dot-directories. Used for monorepo projects with multiple TypeScript packages.

### `findTsConfigForFiles()` — Smart selection [line 79-105]

Selects the best tsconfig for a given set of files:
1. If all files are in the same directory tree → use closest tsconfig to that tree
2. If files are spread across different directories → find common parent, look for tsconfig there
3. Fallback: search from CWD

### `findCommonParentDir()` [line 112-140]

Splits paths into segments, finds the common prefix across all paths.

### Monorepo handling for Rust

For v1: skip `findAllTsConfigs`. Just implement `findTsConfig()` (walk-up). Monorepo support is Phase 4 (doc 0015).

## Extension Matching — Double Extension Bug

**Critical**: Use `path.extname()` (returns `.ts` for both `test.ts` and `dragDrop.test.ts`) rather than `string.endsWith('.ts')` which incorrectly matches `dragDrop.test.ts`.

In Rust: `Path::extension()` returns `Some("ts")` for both `test.ts` and `dragDrop.test.ts`. Always use this for extension comparison. See doc 0014 for full discussion.

## Rust implementation plan

```rust
// lib/file_discovery.rs

use std::path::{Path, PathBuf};
use std::fs;
use glob::glob;

fn collect_files_to_process(
    sources: &[String],
    cwd: &Path,
    extensions: &[String],  // e.g. [".ts", ".tsx"]
    verbose: bool,
) -> Vec<FileEntry> {
    let mut entries: Vec<FileEntry> = Vec::new();

    for src in sources {
        let abs_src = resolve_input_path(src, cwd);

        if abs_src.is_dir() {
            let source_dir_root = abs_src.parent().unwrap().to_path_buf();
            walk_dir(&abs_src, &source_dir_root, extensions, verbose, &mut entries);
        } else if abs_src.is_file() {
            if extensions.iter().any(|ext| abs_src.extension_str() == Some(ext.trim_start_matches('.'))) {
                entries.push(FileEntry {
                    file_path: abs_src,
                    is_directory: false,
                    source_dir_root: None,
                    rel_path_from_source_root: None,
                });
            }
        } else {
            // Glob pattern
            let pattern = Path::new(src);
            let abs_pattern = cwd.join(pattern);
            if let Ok(paths) = glob(abs_pattern.to_str().unwrap()) {
                for entry in paths.flatten() {
                    if entry.is_file() && extensions.iter().any(|ext| entry.extension_str() == Some(ext.trim_start_matches('.'))) {
                        entries.push(FileEntry {
                            file_path: entry,
                            is_directory: false,
                            source_dir_root: None,
                            rel_path_from_source_root: None,
                        });
                    }
                }
            }
        }
    }

    // Deduplicate by file_path
    entries.sort_by_key(|e| e.file_path.clone());
    entries.dedup_by_key(|e| e.file_path.clone());
    entries
}
```

### Dependencies
- `glob` crate (or `fast-glob` equivalent; `glob` is the standard Rust glob crate)
- `walkdir` for recursive directory traversal (more efficient than manual recursion)

### Processing mode thresholds [src/lib/move-files.service.ts:177-184]
```rust
enum ProcessingMode {
    Standard,   // < 15 files
    Surgical,   // 15-34
    Chunked,    // 35-49
    Streaming,  // 50+
}

fn determine_processing_mode(file_count: usize) -> ProcessingMode {
    match file_count {
        0..=14  => ProcessingMode::Standard,
        15..=34 => ProcessingMode::Surgical,
        35..=49 => ProcessingMode::Chunked,
        _       => ProcessingMode::Streaming,
    }
}
```
