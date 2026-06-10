# 0011 - Dependencies and Crates

## Core dependency mapping

| Node.js | Version | Rust Crate | Purpose |
|---------|---------|------------|---------|
| ts-morph | ^19 | `swc_core` (0.10x) | TS/JS parser, AST, codegen |
| commander | ^11 | `clap` 4.x | CLI argument parsing |
| chalk | ^5 | `colored` 2.x | Terminal colors |
| fast-glob | ^3 | `glob` 0.3 | File pattern matching |
| glob | ^11 | `glob` 0.3 | Extended glob support |
| vitest | ^3 | Built-in `#[test]` | Test framework |
| tsup | ^7 | `cargo build` | Build tool |
| typescript | ^5 | N/A (swc handles TS natively) | TypeScript compiler |

## MSRV (Minimum Supported Rust Version)

**1.75.0** (edition 2021, stable). No nightly features needed.

swc_core 0.10+ requires Rust 1.75+. All other crates work on stable.

## Additional Rust crates needed

| Crate | Version | Purpose |
|-------|---------|---------|
| `anyhow` | 1.x | Application-level error handling |
| `thiserror` | 2.x | Library-level error types |
| `serde` | 1.x | tsconfig.json parsing |
| `serde_json` | 1.x | JSON deserialization |
| `walkdir` | 2.x | Recursive directory walking |
| `regex` | 1.x | Import pattern matching (simple updater, critical deps finder) |
| `pathdiff` | 0.2 | `path.relative()` equivalent |
| `tempfile` | 3.x | Temporary directory creation (tests) |
| `assert_cmd` | 2.x | CLI integration testing |
| `predicates` | 3.x | Test condition combinators |
| `tokio` | 1.x | Async runtime (optional, only for streaming in Phase 4) |

## Cargo.toml template

```toml
[package]
name = "tsmv"
version = "0.1.0"
edition = "2021"
description = "Safely move TypeScript files/folders and update imports"
license = "MIT"
repository = "https://github.com/anomalyco/tsmv"
rust-version = "1.75"

# Binary target: CLI tool
[[bin]]
name = "tsmv"
path = "src/main.rs"

# Library target: programmatic API
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
thiserror = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
walkdir = "2"
regex = "1"
pathdiff = "0.2"

[dev-dependencies]
tempfile = "3"
assert_cmd = "2"
predicates = "3"

[profile.release]
opt-level = 2
lto = true
codegen-units = 1
strip = true
```

## swc version note

`swc_core` is the stable API surface. Avoid `swc_ecma_*` crates directly — they are internal and don't follow semver. Use the re-exports from `swc_core`.

Version: check latest on crates.io. At time of writing, `swc_core` 0.10x+ is available. Key: the `ecma_parser`, `ecma_ast`, `ecma_visit`, `ecma_codegen`, and `common` features provide everything needed.

Note: swc_core is NOT needed for Phase 1 (v0.1). The regex-based import updater works without it. Add swc in Phase 3.

## Feature flags strategy

For v1, keep it simple — no feature flags. All functionality is always compiled. If binary size becomes an issue later, consider:
- `regex-only` feature: skips swc, uses regex updater
- `streaming` feature: includes batch processor

## swc API basics for our use case

### Parsing a TypeScript file

```rust
use swc_core::ecma::parser::{Parser, StringInput, Syntax, TsSyntax};
use swc_core::ecma::ast::Module;
use swc_core::common::{FileName, SourceMap, sync::Lrc};

let cm: Lrc<SourceMap> = Default::default();
let fm = cm.new_source_file(FileName::Real(path), source_text.into());

let mut parser = Parser::new(
    Syntax::Typescript(TsSyntax {
        tsx: true,
        decorators: true,
        ..Default::default()
    }),
    StringInput::from(&*fm),
    None,
);

let module: Module = parser.parse_module().unwrap();
```

### Visiting imports

```rust
use swc_core::ecma::visit::{Visit, VisitWith};
use swc_core::ecma::ast::ImportDecl;

struct ImportCollector {
    imports: Vec<ImportDecl>,
}

impl Visit for ImportCollector {
    fn visit_import_decl(&mut self, decl: &ImportDecl) {
        self.imports.push(decl.clone());
    }
}

let mut collector = ImportCollector { imports: vec![] };
module.visit_with(&mut collector);
```

### Modifying imports

```rust
use swc_core::ecma::visit::VisitMut;

struct ImportReplacer {
    // ... mapping data
}

impl VisitMut for ImportReplacer {
    fn visit_mut_import_decl(&mut self, decl: &mut ImportDecl) {
        // modify decl.src.value
    }
}
```

### Code generation (AST → source text)

```rust
use swc_core::ecma::codegen::{text_writer::JsWriter, Emitter};
use swc_core::common::sync::Lrc;
use std::io::BufWriter;

let mut buf = vec![];
{
    let writer = Box::new(JsWriter::new(Lrc::new(cm.clone()), "\n", &mut buf, None));
    let mut emitter = Emitter {
        cfg: Default::default(),
        cm: cm.clone(),
        comments: None,
        wr: writer,
    };
    emitter.emit_module(&module).unwrap();
}
let output = String::from_utf8(buf).unwrap();
```

## swc version note

`swc_core` is the stable API surface. Avoid `swc_ecma_*` crates directly — they are internal and don't follow semver. Use the re-exports from `swc_core`.

Version: check latest on crates.io. At time of writing, `swc_core` 0.10x+ is available.
