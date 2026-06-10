# 0008 - Streaming Batch Processor

Ref files: [src/lib/streaming-processor.service.ts]

## When activated

File count ≥ 50 [src/lib/move-files.service.ts:177-184].

## Why

ts-morph's Project with 50+ files consumes significant memory. Streaming mode creates a fresh Project per batch, processing ~5-10 files at a time, then discarding.

## Architecture

At [src/lib/streaming-processor.service.ts:33-104]:

### `processFilesInBatches()`

```
1. Calculate batch count = ceil(files / batchSize)
2. For each batch:
   a. Call processBatch(batch_slice, destination, options)
   b. Accumulate results (movedFiles, updatedImports, etc.)
   c. global.gc() if available
3. Return aggregated MoveFilesResult
```

### `processBatch()` [line 109-221]

For each batch of ~5-10 files:

1. **Create isolated ts-morph Project** with `useInMemoryFileSystem: true`
   - Tries tsconfig path first
   - Falls back to manual compiler options

2. **Add batch files** to the project

3. **Find critical dependencies** via `findCriticalDependencies()` [line 227-282]:
   - Reads each batch file's content as text
   - Regex extracts all relative imports
   - Resolves each import to absolute path, tries .ts/.tsx/.js/.jsx/index.ts extensions
   - Collects the resolved dependency files

4. **Execute file move:** `executeFileMove(project, batch, destination, options)` — same as non-streaming mode

5. **Clean up:** `sourceFile.forget()` on all project files (releases ts-morph memory)

6. **Return** batch result

### `calculateOptimalBatchSize()` [line 287-311]

```ts
const estimatedMemoryPerFile = 1.5; // MB
const maxBatchByMemory = Math.floor(availableMemoryMB / estimatedMemoryPerFile);

// Scale DOWN as file count goes up
if      (≤50 files)  batchSize = min(10, maxBatchByMemory)
else if (≤100) batchSize = min(8, maxBatchByMemory)
else if (≤200) batchSize = min(6, maxBatchByMemory)
else             batchSize = min(5, maxBatchByMemory)

return max(3, batchSize);  // minimum 3
```

### `getMemoryUsage()` [line 316-324]

```ts
process.memoryUsage() → { heapUsed, heapTotal, external, rss } (in MB)
```

## Rust implementation notes

### Memory management difference

Rust doesn't have a GC; memory is freed when variables go out of scope. The streaming pattern in Rust is simpler:
- Process each batch in its own scope
- Let `Vec<SourceFile>` drop when scope ends
- Each batch gets a fresh AST parsing cycle

```rust
async fn process_files_in_batches(
    files: &[PathBuf],
    destination: &Path,
    options: &StreamingOptions,
) -> Result<MoveFilesResult> {
    let batch_size = calculate_optimal_batch_size(files.len());
    let mut accumulator = MoveFilesResult::default();

    for chunk in files.chunks(batch_size) {
        {
            // Isolated scope for memory cleanup
            let result = process_batch(chunk, destination, options)?;
            accumulator.merge(result);
        }
        // chunk, project, ast trees dropped here
    }

    Ok(accumulator)
}
```

### `findCriticalDependencies()` in Rust

Same approach: read file as string, regex for imports, resolve paths:

```rust
use regex::Regex;

fn find_critical_dependencies(
    batch_files: &[PathBuf],
) -> Vec<PathBuf> {
    let import_re = Regex::new(
        r#"import\s+.*?\s+from\s+['"](.*?)['"]"#
    ).unwrap();

    let mut deps = HashSet::new();
    for file in batch_files {
        let content = std::fs::read_to_string(file).unwrap_or_default();
        for cap in import_re.captures_iter(&content) {
            let import_path = &cap[1];
            if import_path.starts_with('.') {
                let resolved = file.parent().unwrap().join(import_path);
                // Try extensions: .ts, .tsx, .js, .jsx, index.ts
                for ext in &[".ts", ".tsx", ".js", ".jsx"] {
                    let p = Path::new(&format!("{}{}", resolved.display(), ext));
                    if p.exists() { deps.insert(p.to_path_buf()); break; }
                }
                let index = resolved.join("index.ts");
                if index.exists() { deps.insert(index); }
            }
        }
    }
    deps.into_iter().collect()
}
```

### Batch size calculation

Rust version can be simpler (no memory estimation from OS):
- Configurable default (e.g. 10)
- Optional: detect available system memory via `sysinfo` crate for dynamic sizing
