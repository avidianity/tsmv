# 0013 - Review: Gaps, Discrepancies, and Fixes

## Discrepancy 1: Import update flow is wrong in 0001

**Doc 0001 says** `executeFileMove()` calls `planImportUpdates()` → `executeImportUpdates()`.

**Actual code** ([src/lib/file-operations.service.ts:310-360]): `executeFileMove()` calls `planFileOperations()` → `executeFileOperations()` → `project.saveSync()` → `cleanupEmptyDirectories()`. It does NOT call `planImportUpdates()` or `executeImportUpdates()`. Those functions exist but are unused in the main flow.

**Why**: ts-morph's `sourceFile.move(dest)` (called in `executeFileOperations()`) handles import path updates WITHIN the ts-morph project automatically. The separate `planImportUpdates()`/`executeImportUpdates()` were likely an earlier approach or a utility for manual import correction.

**Fix**: 0001 data flow should show that import updates happen inside ts-morph's `move()`, not via separate functions.

## Discrepancy 2: `syntax-aware-import-updater` not wired into main flow

**Doc 0001 lists it as HIGH priority.** But `syntax-aware-import-updater` is NEVER imported by `move-files.service.ts`. It is only:
- Exported from `src/lib/syntax-aware-import-updater/index.ts`
- NOT re-exported from `src/lib/index.ts`
- NOT called anywhere in the `moveFiles()` orchestrator

It's a standalone module that does its own complete import update pipeline (refresh project, add moved files, detect circular deps, update imports, save). Its logic partially overlaps with `file-operations.service.ts`.

**Fix**: Note that synta-aware-updater is an alternative/standalone module, not part of the main pipeline. In Rust port, merge its logic into the single import update flow (don't maintain two parallel implementations).

## Discrepancy 3: `simple-import-updater.service.ts` is orphaned

**Doc 0001 lists it as MED priority.** But `simple-import-updater.service.ts` is NEVER imported by any other module. It's a fully independent regex-based approach:

- `updateImportsInProject()` takes a `Map<oldPath, newPath>` and scans ALL project files with regex
- No ts-morph dependency — pure string manipulation
- No tsconfig awareness
- No path alias support
- No circular dependency detection

**Fix**: Clarify it's an INDEPENDENT fallback. For Rust v1, the regex approach from this module is actually a good starting point (simpler than swc AST). Could implement this first, then upgrade to swc.

## Gap 1: No coverage of edge cases from tests

Test files that exist in the repo cover important edge cases not mentioned in any doc:

| Test | Edge case |
|------|-----------|
| `double-extension-bug.test.ts` | Files like `*.test.ts` — the extension `.test.ts` vs `.ts` resolution. Extension matching at [src/lib/move-files.service.ts:87] uses `fullPath.endsWith(ext)` which could mis-match `.ts` against `.test.ts`. The fix uses `path.extname()` comparison instead of simple endsWith. |
| `self-import-preservation.test.ts` | Files that re-export or import from themselves (e.g., barrel `index.ts` files). Moving these must not create broken self-references. |
| `stack-overflow-protection.test.ts` | Deeply nested JSX/objects triggering stack overflow in ts-morph parser. Safe parser ([src/lib/safe-parser.service.ts]) was specifically built for this. |
| `deeplyNestedMigration.test.ts` | Moving files buried 4+ directories deep. Tests relative path calculation for `../../../../etc`. |
| `large-scale-operations.test.ts` | 100+ files triggering streaming mode. Tests batch sizing and memory behavior. |
| `arc7-protocol-test.test.ts` | ARC-7 protocol compliance: batch operations, state management, circular deps, and the full protocol spec. |

## Gap 2: Test fixture not documented

`tests/complex-document-editor-migration/` is a realistic React app fixture with:
- Multiple component types (organisms, molecules, atoms)
- Style files (*.styles.ts)
- Logic files (*.logic.ts)
- Hook files (*.hook.ts)
- Type files (*.types.ts)
- Storybook stories (*.stories.tsx)
- Barrel index.ts files
- Deep nesting (5+ levels)

**Fix**: Add to 0012 as a "real-world test scenario" that should be replicated for integration testing.

## Gap 3: No documentation of legacy/dead code

Three services are legacy or unused:

| File | Status |
|------|--------|
| `src/lib/file-handler.service.ts` | Legacy `handleFileMove()` — replaced by `file-operations.service.ts`. Has `--recursive` flag support that the new code may lack. |
| `src/lib/path-updater.service.ts` | Thin wrappers: `updateImports()` + `refreshProjectReferences()`. Only 41 lines. Unused in main flow. |
| `src/lib/exec-move-command.service.ts` + `.js` | Wraps `execSync('mv ...')`. A fallback for when ts-morph can't be used. The `.js` variant suggests this predates TypeScript conversion. |
| `src/lib/execMoveCommand.js` | Duplicate of above, raw JavaScript. |

**Fix**: Add a section to 0001 noting which files are carry-over/legacy and can be skipped in Rust port.

## Gap 4: No doc for `commands/utils.ts`

The tsconfig discovery helpers (`findTsConfig`, `findAllTsConfigs`, `findTsConfigForFiles`, `findCommonParentDir`) are ~140 lines [src/commands/utils.ts] and used by `createProjectConfig()` in [src/lib/move-files.service.ts:189-250]. They're mentioned briefly in 0001 but deserve their own treatment.

**Fix**: Add brief section to 0003 or a separate doc about tsconfig discovery (walk-up algorithm, monorepo support, fallback behavior).

## Gap 5: Absolute imports in streaming mode uses different approach

In streaming mode [src/lib/move-files.service.ts:378-420], absolute import conversion creates a SEPARATE temp ts-morph Project that loads only moved files from disk (not the in-memory batch project). This matters for Rust implementation — absolute imports conversion is always a post-move, fresh-parse operation.

## Gap 6: No mention of `--no-absolute-imports` flag

The `absoluteImports` option defaults to `true` [src/lib/move-files.service.ts:574]: `const shouldConvertToAbsolute = options.absoluteImports !== false;`. Users can opt out with `--no-absolute-imports`. This is not documented in any plan doc. Relevant for CLI option spec.

## Gap 7: Missing Rust Cargo.toml details

Doc 0011 lists crate names but not:
- Minimum supported Rust version (MSRV)
- Whether to target stable or nightly
- Feature flags needed
- Binary vs library target setup

## Summary: What to fix

| Doc | Fix |
|-----|-----|
| 0001 | Correct data flow (imports handled by ts-morph move(), not separate functions). Mark legacy files. |
| 0005 | Note that synta-aware-updater is standalone, not wired to main flow. Merge into single pipeline for Rust. |
| 0006 | Remove `planImportUpdates()`/`executeImportUpdates()` from flow — they exist but unused. |
| NEW 0013 | Edge cases doc: double extension bug, self-imports, stack overflow, deep nesting |
| 0003 | Add section on tsconfig discovery algorithm |
| 0011 | Add MSRV, feature flags, binary/lib setup |
| 0002 | Add `--no-absolute-imports` to CLI option spec |
| 0012 | Add complex-document-editor-migration test fixture reference |
