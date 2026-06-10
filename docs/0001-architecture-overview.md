# 0001 - Architecture Overview

## High-level data flow

```
CLI args (commander)
  → moveAction()                  [src/commands/move.ts]
    → moveFiles()                 [src/lib/move-files.service.ts]  <-- central orchestrator
      ├→ collectFilesToProcess()  [src/lib/move-files.service.ts:58]  (glob + fs walk)
      ├→ createProjectConfig()    [src/lib/move-files.service.ts:189] (tsconfig resolution)
      ├→ determineProcessingMode() [src/lib/move-files.service.ts:177] (file count → mode)
      ├─ streaming mode (50+ files):
      │   └→ processFilesInBatches() [src/lib/streaming-processor.service.ts:33]
      │       └→ processBatch() × N  [src/lib/streaming-processor.service.ts:109]
      │           └→ executeFileMove() [src/lib/file-operations.service.ts:310]
      ├─ traditional modes (<50 files):
      │   ├→ collectImportContexts() [src/lib/move-files.service.ts:255]
      │   ├→ executeDryRun()        [src/lib/dry-run.service.ts:99]  (if -n flag)
      │   └→ executeFileMove()      [src/lib/file-operations.service.ts:310]
      │       ├→ planFileOperations()    [src/lib/file-operations.service.ts:35]
      │       ├→ executeFileOperations() [src/lib/file-operations.service.ts:156]
      │       │   └→ sourceFile.move()   (ts-morph move — renames file on disk + updates internal refs)
      │       ├→ project.saveSync()      [src/lib/file-operations.service.ts:328] (single commit)
      │       └→ cleanupEmptyDirectories() [src/lib/file-operations.service.ts:254]
      │       
      │       NOTE: planImportUpdates()/executeImportUpdates() [src/lib/file-operations.service.ts:109-249]
      │       are NOT called in the main flow. ts-morph's sourceFile.move() handles import path updates
      │       automatically within its project model. These functions are standalone utilities.
      └→ convertProjectToAbsoluteImports() [src/lib/absolute-imports.service.ts:204]
          ├→ parsePathAliases()    [src/lib/absolute-imports.service.ts:17]
          └→ updateImportsToAbsolute() × N [src/lib/absolute-imports.service.ts:143]
```

## Source file inventory

| File | Purpose | Rust priority |
|------|---------|---------------|
| `src/index.ts` | CLI entry point, commander setup | HIGH |
| `src/exports.ts` | Library exports (programmatic API) | HIGH |
| `src/cli-install-rules.ts` | Cursor rules installer | SKIP (Cursor-specific, not needed) |
| `src/commands/move.ts` | moveAction - validates + delegates | HIGH |
| `src/commands/utils.ts` | tsconfig.json discovery helpers | MED |
| `src/types/index.d.ts` | MoveOptions, MovedFilesMap types | HIGH |
| `src/lib/move-files.service.ts` | Central orchestrator (616 lines) | HIGH |
| `src/lib/file-operations.service.ts` | Plan + execute file moves + import updates | HIGH |
| `src/lib/import-path.service.ts` | Relative path calculation, import resolution | HIGH |
| `src/lib/absolute-imports.service.ts` | Relative→absolute import conversion | MED |
| `src/lib/safe-parser.service.ts` | Stack-safe BFS AST traversal | HIGH |
| `src/lib/dry-run.service.ts` | Dry-run preview generation | MED |
| `src/lib/streaming-processor.service.ts` | Batch processing for 50+ files | MED |
| `src/lib/simple-import-updater.service.ts` | Regex-based import updater (lighter) | MED |
| `src/lib/path-updater.service.ts` | ts-morph-specific file move helper | SKIP (legacy, 41 lines, unused in flow) |
| `src/lib/file-handler.service.ts` | Legacy file handler (superseded) | SKIP (163 lines, replaced by file-operations) |
| `src/lib/exec-move-command.service.ts` | Wraps Unix `mv` command | SKIP (fallback, unused in main flow) |
| `src/lib/execMoveCommand.js` | Duplicate of exec-move-command (.js) | SKIP (JavaScript duplicate) |
| `src/lib/syntax-aware-import-updater/` | Sub-module: syntax-aware import updates | MED (standalone, NOT wired to main flow — see review doc 0013) |
| `src/lib/syntax-aware-import-updater/import-resolver.service.ts` | Path matching, relative path calc | MED (duplicates import-path.service.ts logic) |
| `src/lib/syntax-aware-import-updater/circular-dependency-detector.service.ts` | DFS cycle detection | MED |
| `src/lib/syntax-aware-import-updater/global-move-tracker.service.ts` | Global move history (mutable state) | LOW |
| `src/lib/syntax-aware-import-updater/syntax-aware-import-updater.types.ts` | Type definitions | MED |
| `src/lib/syntax-aware-import-updater/syntax-aware-import-updater.service.ts` | Main import update pipeline | MED (standalone) |

## Legacy / Unused Files

These exist in the codebase but are NOT part of the main `moveFiles()` flow:

| File | Why skip |
|------|----------|
| `src/lib/file-handler.service.ts` | Older implementation. `handleFileMove()` does manual recursive directory walking + fs.rename. Superseded by `file-operations.service.ts` which uses ts-morph's managed file operations. |
| `src/lib/path-updater.service.ts` | Tiny wrapper (41 lines): `updateImports()` calls `sourceFile.move()`, `refreshProjectReferences()` calls `project.resolveSourceFileDependencies()`. Neither called by any other module. |
| `src/lib/exec-move-command.service.ts` + `execMoveCommand.js` | Simple `execSync('mv ...')` wrapper. Fallback for when ts-morph not available. The `.js` file is a pre-TypeScript remnant. |
| `src/lib/simple-import-updater.service.ts` | Regex-based import updater. Never imported by any other module. Is an independent utility — could serve as basis for Rust v1 regex approach before implementing swc AST. |

## Processing modes

| Mode | File count | Strategy |
|------|-----------|----------|
| standard | 0-14 | Full ts-morph project, all files loaded |
| surgical | 15-34 | Selective file loading |
| chunked | 35-49 | Moderate batching |
| streaming | 50+ | Batched ts-morph projects, GC between batches |

Ref: [src/lib/move-files.service.ts:177-184]

## Dependencies → Rust equivalents

| Node.js dependency | Purpose | Rust crate |
|-------------------|---------|------------|
| `ts-morph` (TypeScript compiler API wrapper) | AST parsing, import manipulation, file management | `swc` (swc_core) |
| `commander` | CLI argument parsing | `clap` + `clap_complete` |
| `chalk` | Colored terminal output | `colored` or `console` |
| `fast-glob` | Glob file matching | `glob` |
| `glob` (v11) | Additional glob features | `glob` |
| `vitest` | Test framework | Built-in `cargo test` |
| `tsup` | TypeScript bundler | N/A (cargo build) |

## Key design decisions for Rust port

1. **swc instead of ts-morph**: swc is a Rust-native TypeScript/JavaScript compiler. Use `swc_ecma_parser` for parsing, `swc_ecma_ast` for AST types, `swc_ecma_visit` for traversal, `swc_ecma_codegen` for code generation, and `swc_common` for source maps / file management.

2. **Error handling**: Use `anyhow` for application-level errors, `thiserror` for library-level. Pattern: central `Result<T, TsmvError>` type.

3. **Async**: Use `tokio` runtime for async file I/O where beneficial (streaming mode, project-wide file scanning). Sync is fine for small file sets.

4. **No global mutable state**: The global move tracker in [src/lib/syntax-aware-import-updater/global-move-tracker.service.ts:11] is a simple `Vec<FileMoveMapping>`. Pass state explicitly via function arguments instead.

5. **File system**: `std::fs` for most operations. `walkdir` crate for recursive directory traversal.
