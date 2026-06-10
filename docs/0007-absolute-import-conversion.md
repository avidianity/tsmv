# 0007 - Absolute Import Conversion

Ref files: [src/lib/absolute-imports.service.ts]

## Purpose

After moving files, optionally convert all relative imports to absolute imports using tsconfig path aliases. Example:

```ts
// Before (relative)
import { Button } from '../components/Button';

// After (absolute)
import { Button } from '@/components/Button';
```

## When it runs

At [src/lib/move-files.service.ts:574-613]:
- Default: ON (`absoluteImports` !== false)
- Requires `tsconfig.json` to be found (has `compilerOptions.paths` or uses defaults)
- Skipped if `--no-absolute-imports` flag or no tsconfig

## Key functions

### `parsePathAliases()` [line 17-68]

Reads tsconfig, extracts:
- `compilerOptions.baseUrl` (default `./src`)
- `compilerOptions.paths` (alias → path array mappings)

If no paths defined, uses hardcoded defaults [line 24-31]:
```
@/*          → ./src/*
@/shared/*   → ./src/shared/*
@/features/* → ./src/features/*
@/components/* → ./src/components/*
@/utils/*    → ./src/utils/*
```

### `convertToAbsoluteImport()` [line 73-138]

Algorithm:
1. If already starts with `@` → skip
2. If not a relative import (`!startsWith('.')`) → skip (node_modules)
3. Resolve import path to absolute file path
4. Make relative to project root
5. Normalize path separators
6. For each alias pattern, check if normalized path matches the target pattern
7. If match → substitute alias pattern with matched wildcard
8. If no specific match → fallback: `@/restOfPath` (strip `src/` prefix if present)

### `updateImportsToAbsolute()` [line 143-199]

For a single source file:
- Iterate `importDeclarations` → convert each module specifier
- Also handle `exportDeclarations` (re-exports)
- Returns count of conversions

### `convertProjectToAbsoluteImports()` [line 204-235]

Orchestrator: runs `updateImportsToAbsolute()` on all source files.

## Rust implementation notes

tsconfig parsing: direct JSON parsing with `serde_json`. No need for the TypeScript compiler.

```rust
use serde::Deserialize;

#[derive(Deserialize)]
struct TsConfig {
    #[serde(rename = "compilerOptions")]
    compiler_options: Option<CompilerOptions>,
}

#[derive(Deserialize)]
struct CompilerOptions {
    #[serde(rename = "baseUrl")]
    base_url: Option<String>,
    paths: Option<HashMap<String, Vec<String>>>,
}

struct PathAliasConfig {
    prefix: String,           // e.g. "@"
    base_url: String,         // e.g. "./src"
    paths: HashMap<String, String>,  // e.g. "@/*" → "./src/*"
}
```

### Path matching logic [line 102-118]

```rust
fn convert_to_absolute(
    relative_import: &str,
    source_file: &Path,
    aliases: &PathAliasConfig,
    project_root: &Path,
) -> String {
    if !relative_import.starts_with('.') {
        return relative_import.to_string();
    }

    let source_dir = source_file.parent().unwrap();
    let resolved = source_dir.join(relative_import);
    let relative_to_root = pathdiff::diff_paths(&resolved, project_root)
        .unwrap_or(resolved.clone());

    let normalized = relative_to_root.to_str().unwrap().replace('\\', "/");

    // Try specific alias patterns first
    for (alias_pattern, target_pattern) in &aliases.paths {
        let clean_target = target_pattern.replace("*", "$1").trim_start_matches("./").to_string();
        if normalized.starts_with(&clean_target) {
            let matched_part = &normalized[clean_target.len()..];
            return alias_pattern.replace('*', matched_part);
        }
    }

    // Fallback: general @ prefix
    let cleaned = normalized.strip_prefix("src/").unwrap_or(&normalized);
    format!("{}/{}", aliases.prefix, cleaned)
}
```

### AST modification approach

Same as doc 0005: use `swc_ecma_visit::VisitMut` to walk `ImportDecl` and `ExportDecl` nodes, replace `src.value` with converted path.

### Memory consideration

In streaming mode (>50 files), absolute import conversion runs as a separate pass AFTER all file moves complete [src/lib/move-files.service.ts:378-420]. It creates a fresh ts-morph Project, adds moved files, converts, saves, and cleans up. Same pattern in Rust: separate pass with fresh AST parsing.
