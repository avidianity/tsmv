# 0012 - Testing Strategy

Ref files: [tests/unit/], [tests/integration/], [tests/e2e/]

## Original test structure

| Test type | Location | Framework | What it tests |
|-----------|----------|-----------|---------------|
| Unit | `tests/unit/fileHandler.test.ts` | vitest + mocks | `handleFileMove()`, `execMoveCommand()` |
| Integration | `tests/integration/*.test.ts` | vitest + real FS | `moveFiles()` with real temp directories |
| E2E | `tests/e2e/*.test.ts` | vitest + execSync | CLI invocation, real file moves |
| Setup | `tests/setup.ts` | vitest | Test environment config |

## Key integration test patterns

### `tests/integration/moveWithImports.test.ts` [line 1-96]

Pattern: create temp directory, set up TypeScript files, run moveFiles, verify:
1. File moved to new location
2. Import statements in other files updated correctly

```ts
it('should move a file and update imports', async () => {
    await moveFiles([path.join(utilsDir, 'helpers.ts')], sharedDir, { force: true });
    expect(fs.existsSync(path.join(sharedDir, 'helpers.ts'))).toBe(true);
    expect(fs.existsSync(path.join(utilsDir, 'helpers.ts'))).toBe(false);
    // Check import was updated from '../utils/helpers' to '@/shared/helpers'
});
```

### `tests/e2e/cli.test.ts` [line 1-100+]

Runs actual CLI binary via `execSync`:
```ts
const cmd = `pnpm tsx bin/index.ts --verbose -f ${src} ${dest}`;
const output = execSync(cmd, { encoding: 'utf-8' });
```

Then checks:
- File exists at new location
- File gone from old location
- Import paths updated in referencing files
- CLI exit codes (0 for success, non-zero for errors)
- Output messages match expected patterns

## Rust test strategy

### Cargo project layout

```
src/
  main.rs
  lib.rs              // re-exports public API
  commands/
  lib/
    mod.rs
    file_discovery.rs
    move_files.rs
    file_operations.rs
    import_path.rs
    import_updater.rs    // swc-based
    simple_import_updater.rs  // regex-based
    absolute_imports.rs
    streaming.rs
    dry_run.rs
    circular_deps.rs
tests/
  integration/
    move_with_imports.rs
    directory_preservation.rs
    deeply_nested.rs
    self_import_preservation.rs
    double_extension_bug.rs
  e2e/
    cli_test.rs
```

### Unit tests (in-file)

Rust convention: unit tests live in the same file as the code they test, in a `#[cfg(test)] mod tests {}` block.

```rust
// src/lib/import_path.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_relative_path_same_dir() {
        // ...
    }
}
```

### Integration tests

Use `tempfile::TempDir` for isolated test environments:

```rust
// tests/integration/move_with_imports.rs
use tempfile::TempDir;

#[test]
fn test_move_file_updates_imports() -> Result<()> {
    let dir = TempDir::new()?;

    // Set up test files
    let src = dir.path().join("src");
    std::fs::create_dir_all(&src)?;
    std::fs::write(src.join("a.ts"), "export const a = 1;")?;
    std::fs::write(src.join("b.ts"), "import { a } from './a';")?;

    // Create tsconfig
    std::fs::write(dir.path().join("tsconfig.json"), r#"{"compilerOptions":{"target":"es2020"}}"#)?;

    // Run move
    tsmv::move_files(
        &[src.join("a.ts")],
        src.join("utils").as_path(),
        &MoveOptions::default(),
    )?;

    // Verify
    assert!(src.join("utils").join("a.ts").exists());
    assert!(!src.join("a.ts").exists());
    let b_content = std::fs::read_to_string(src.join("b.ts"))?;
    assert!(b_content.contains("from './utils/a'"));
    Ok(())
}
```

### E2E tests

Use `assert_cmd` for CLI testing:

```rust
// tests/e2e/cli_test.rs
use assert_cmd::Command;

#[test]
fn test_cli_move_basic() {
    let dir = tempfile::TempDir::new().unwrap();
    // ... set up files ...

    let mut cmd = Command::cargo_bin("tsmv").unwrap();
    cmd.args(["--force", "src/a.ts", "src/utils/"])
        .current_dir(dir.path());

    let assert = cmd.assert().success();
    let output = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(output.contains("Extracted sources"));
}
```

## Test cases to port from original

### From `tests/integration/`

1. **moveWithImports.test.ts** — basic file move + import update
2. **advancedMoves.test.ts** — multiple files, edge cases
3. **deeplyNestedMigration.test.ts** — deep directory structures
4. **stack-overflow-protection.test.ts** — large/complex files
5. **self-import-preservation.test.ts** — files importing themselves
6. **double-extension-bug.test.ts** — `.test.ts` → `.ts` resolution bug
7. **large-scale-operations.test.ts** — 50+ files, streaming mode

### From `tests/e2e/`

1. **cli.test.ts** — basic CLI move command
2. **cli-subdir-relative-move.test.ts** — subdirectory structure preservation
3. **arc7-protocol-test.test.ts** — comprehensive edge cases per ARC-7 protocol

### From `tests/complex-document-editor-migration/`

Large test fixture with a realistic React app structure (200+ files):
- Component hierarchy: organisms → molecules → atoms
- Per-component files: `Component.types.ts`, `Component.tsx`, `Component.styles.ts`, `Component.logic.ts`, `Component.hook.ts`, `Component.stories.tsx`
- Barrel `index.ts` re-exports
- Deep nesting (5+ levels)
- Cross-component imports at every level
- Token files, template files, hook files

**Rust**: Replicate this fixture in `tests/fixtures/complex-app/` for integration testing. Tests that:
1. Moving a leaf component updates all barrel files above it
2. Moving a molecule updates all organisms that import it
3. Moving a shared token updates all consumers
4. Deep nesting doesn't break path calculation

## Test runner

```bash
cargo test                    # All tests
cargo test --test integration # Integration tests only
cargo test --test e2e         # E2E tests only
cargo test import_path        # Run specific test module
```
