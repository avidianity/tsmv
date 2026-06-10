# 0015 - Setup Plan

## Phase 0: Project Bootstrap

### Step 0.1: Initialize Cargo project
```
cargo init /home/avidian/Development/tsmv
```

### Step 0.2: Cargo.toml

```toml
[package]
name = "tsmv"
version = "0.1.0"
edition = "2021"
description = "Safely move TypeScript files/folders and update imports"
license = "MIT"
repository = "https://github.com/anomalyco/tsmv"

[[bin]]
name = "tsmv"
path = "src/main.rs"

[lib]
name = "tsmv"
path = "src/lib.rs"

[dependencies]
clap = { version = "4", features = ["derive"] }
swc_core = { version = "0.10", features = [
    "ecma_parser",
    "ecma_ast",
    "ecma_visit",
    "ecma_codegen",
    "common",
] }
colored = "2"
glob = "0.3"
anyhow = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
walkdir = "2"
regex = "1"
pathdiff = "0.2"

[dev-dependencies]
tempfile = "3"
assert_cmd = "2"
predicates = "3"
```

### Step 0.3: Directory structure

```
src/
  main.rs                 # CLI entry: clap setup, dispatch
  lib.rs                  # Public API re-exports
  cli.rs                  # Arg parsing types
  options.rs              # MoveOptions struct
  errors.rs               # Error types (thiserror + anyhow)
  lib/
    mod.rs
    move_files.rs         # Main orchestrator (core of move-files.service.ts)
    file_discovery.rs     # collectFilesToProcess + walk helpers
    file_operations.rs    # executeFileMove, plan/execute/cleanup
    import_path.rs        # resolveImportPath, calculateRelativePath
    import_updater.rs     # swc-based AST import updates
    regex_updater.rs      # Regex-based simple updater (v1 fallback)
    absolute_imports.rs   # Path alias conversion
    streaming.rs          # Batch processor for 50+ files
    dry_run.rs            # Preview mode
    circular_deps.rs      # DFS cycle detection
    tsconfig.rs           # tsconfig.json parsing + discovery
tests/
  integration/
    move_basic.rs
    move_directory.rs
    move_deep.rs
    double_extension.rs
    self_import.rs
  e2e/
    cli_test.rs
  common/
    mod.rs                # Test utilities: create_temp_project(), etc.
```

### Step 0.4: Error handling (src/errors.rs)

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TsmvError {
    #[error("No files matched the provided patterns")]
    NoFilesMatched,

    #[error("Source file not found: {0}")]
    SourceNotFound(String),

    #[error("Destination already exists: {0} (use --force to overwrite)")]
    DestinationExists(String),

    #[error("Cannot move directory without --recursive flag: {0}")]
    RecursiveRequired(String),

    #[error("swc parse error: {0}")]
    ParseError(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Generic(String),
}

pub type Result<T> = std::result::Result<T, anyhow::Error>;
```

## Phase 1: Core Pipeline (v0.1)

Implement the minimal viable program:

| Step | Module | What |
|------|--------|------|
| 1.1 | `main.rs` + `cli.rs` | Clap CLI with `move` subcommand + default command |
| 1.2 | `options.rs` | `MoveOptions` struct (all CLI flags) |
| 1.3 | `file_discovery.rs` | File collection: resolve paths, walk directories, glob matching |
| 1.4 | `tsconfig.rs` | Find + parse tsconfig.json (walk-up algorithm) |
| 1.5 | `import_path.rs` | `calculateRelativePath()`, `resolveImportTarget()`, `pathsMatch()` |
| 1.6 | `file_operations.rs` | `executeFileMove()`: plan ops → execute (fs::rename) → cleanup dirs |
| 1.7 | `regex_updater.rs` | Regex-based import updater (v1 — simpler than swc) |
| 1.8 | `move_files.rs` | Orchestrator: collect → validate → move → update imports |

**V1 approach**: use regex updater first (port of `simple-import-updater.service.ts`). It works correctly for all relative import patterns and is 200 lines vs swc's complexity. Add swc-based updater in Phase 3.

### Phase 1 exit criteria:
- `tsmv file.ts destDir/` moves file and updates imports
- `tsmv -n file.ts destDir/` shows dry-run preview
- `tsmv --force file.ts destDir/` overwrites
- `tsmv dir1/ dir2/ destDir/` moves multiple items
- All integration tests from phase 1 pass

## Phase 2: Directory Moves & Edge Cases (v0.2)

| Step | What |
|------|------|
| 2.1 | Directory structure preservation (sourceDirRoot logic) |
| 2.2 | `--recursive` flag |
| 2.3 | Double extension handling (`.test.ts`) |
| 2.4 | Self-import preservation |
| 2.5 | Index file resolution (`./utils` → `./utils/index.ts`) |
| 2.6 | `--interactive` flag (prompt before overwrite) |
| 2.7 | `--extensions` flag for non-.ts files |

## Phase 3: AST-Based Import Updates (v0.3)

| Step | What |
|------|------|
| 3.1 | swc parser integration (parse TS/TSX files) |
| 3.2 | `import_updater.rs`: AST-based import modification with VisitMut |
| 3.3 | Code generation: emit modified AST to string |
| 3.4 | Preserve original formatting (trailing commas, quotes, semicolons) |
| 3.5 | Make regex vs AST updater selectable (flag or file count threshold) |

## Phase 4: Advanced Features (v0.4+)

| Step | What |
|------|------|
| 4.1 | Streaming mode (50+ files, batched processing) |
| 4.2 | Absolute import conversion (path aliases from tsconfig) |
| 4.3 | Circular dependency detection (DFS warning) |
| 4.4 | Monorepo support (multiple tsconfig.json) |
| 4.5 | `--backup` flag (simple/numbered backups) |
| 4.6 | Dry-run shows exact import changes (parse project files) |

## Phase 5: Polish (v1.0)

| Step | What |
|------|------|
| 5.1 | Full E2E test suite against realistic project fixtures |
| 5.2 | Cross-platform testing (Windows path separators) |
| 5.3 | Performance benchmarks vs original TypeScript tool |
| 5.4 | Shell completions (clap_complete for bash/zsh/fish) |
| 5.5 | CI/CD pipeline (GitHub Actions) |
| 5.6 | Release to crates.io |

## Immediate Next Actions

1. `cargo init` + write Cargo.toml
2. Create directory structure
3. Write `errors.rs`, `options.rs`, `cli.rs`
4. Write `main.rs` with clap
5. Write `file_discovery.rs` (no deps on swc)
6. Write `import_path.rs` (pure path math, no deps)
7. Write `regex_updater.rs` (depends on regex crate only)
8. Write `file_operations.rs` (depends on file_discovery + import_path)
9. Write `move_files.rs` orchestrator
10. Write first integration test

### Estimated effort per phase

| Phase | Files | Lines (est) | Complexity |
|-------|-------|-------------|------------|
| 0     | 5     | ~200        | Low |
| 1     | 8     | ~1200       | Medium |
| 2     | adds ~300 | ~300     | Medium |
| 3     | 2     | ~400        | High (swc learning curve) |
| 4     | 4     | ~600        | Medium-High |
| 5     | varies | ~500        | Medium |
