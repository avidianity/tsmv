# 0002 - CLI Entry Point

Ref files: [src/index.ts], [src/commands/move.ts]

## Current structure (commander)

`src/index.ts` sets up a `commander` program with:

1. **Default command** (no subcommand) — takes `<args...>` where last arg is destination, rest are sources
2. **`move` subcommand** — identical behavior to default command
3. **`install-rules` subcommand** — installs Cursor AI rules (skip for Rust v1)

### Options (applied to both default and `move`)

```
-r, --recursive      Recursively move directories
-i, --interactive    Prompt before overwrite
-f, --force          Force overwrite without prompt
-n, --dry-run        Preview only, no changes
-v, --verbose        Display detailed operation logs
--extensions <ext>   File extensions to consider (comma-separated, default: .ts,.tsx)
--tsconfig <path>    Path to tsconfig.json
--no-absolute-imports  Disable relative→absolute import conversion (enabled by default)
--absolute-imports   Enable relative→absolute import conversion (default)
--alias-prefix <p>   Alias prefix for absolute imports (default: @)
```

### Arg parsing pattern

```
ts-import-move [options] <source1> [source2...] <destination>
ts-import-move move [options] <source1> [source2...] <destination>
```

Last argument = destination. All previous = sources. Implemented at [src/index.ts:72-82] and [src/index.ts:101-111].

## Rust port plan (clap)

```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "tsmv", version, about = "Safely move TypeScript files/folders and update imports")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    #[arg(short = 'r', long, help = "Recursively move directories")]
    recursive: bool,

    #[arg(short = 'i', long, help = "Prompt before overwrite")]
    interactive: bool,

    #[arg(short = 'f', long, help = "Force overwrite without prompt")]
    force: bool,

    #[arg(short = 'n', long, help = "Show what would be moved without making changes")]
    dry_run: bool,

    #[arg(short = 'v', long, help = "Display detailed operation logs")]
    verbose: bool,

    #[arg(long, default_value = ".ts,.tsx", help = "File extensions to consider (comma separated)")]
    extensions: String,

    #[arg(long, help = "Path to tsconfig.json")]
    tsconfig: Option<String>,

    /// Source file(s) and destination (last element is destination)
    #[arg(required = true, num_args = 2..)]
    args: Vec<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Move TypeScript files/folders and update imports
    Move {
        /// Source file(s) and destination (last element is destination)
        #[arg(required = true, num_args = 2..)]
        args: Vec<String>,
    },
}
```

## Key behavior notes

- Both default and `move` subcommand produce identical output (Extracted sources/destination logging) — see [src/index.ts:76-77]
- Version loaded from package.json at [src/index.ts:30-39]. Rust: use `env!("CARGO_PKG_VERSION")`.
- `__dirname` computation for ESM/CJS compatibility [src/index.ts:15-27] not needed in Rust.
- The `moveAction()` in [src/commands/move.ts:8-39] is a thin wrapper: validates, logs DEBUG, delegates to `moveFiles()`.

## Rust file structure plan

```
src/
  main.rs              -- CLI entry point (clap setup, arg parsing)
  commands/
    mod.rs
    move.rs            -- move_action() equivalent
  lib.rs               -- library root, re-exports public API
```

`commands/move.rs` mirrors [src/commands/move.ts] — validates args, then hands off to the orchestrator in `lib/move_files.rs`.
