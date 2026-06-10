# 0014 - Edge Cases

This doc captures edge cases from the test suite that the Rust port must handle. Ref tests: `tests/integration/*.test.ts`, `tests/e2e/*.test.ts`.

## 1. Double Extension Bug

**Test**: `tests/integration/double-extension-bug.test.ts`

**Problem**: Files like `logic.test.ts` or `styles.test.ts` — the extension `.ts` matches `.test.ts` when using simple `endsWith('.ts')` comparison. This can cause:
- Wrong file type classification
- Incorrect extension stripping in import paths (e.g., `import './dragDrop.test'` → strip `.test` as extension)

**Fix in original**: [src/lib/move-files.service.ts:87] and [src/lib/import-path.service.ts:76] use `path.extname()` which correctly returns `.ts` for `.test.ts` files (the extension is always the LAST `.ext`). The import path stripping regex [src/lib/import-path.service.ts:76]: `toFile.replace(/\.(ts|tsx|js|jsx)$/, '')` — the `$` anchor ensures only the final extension is stripped.

**Rust**: Use `Path::extension()` which returns `Some("ts")` for `test.ts` and `Some("ts")` for `dragDrop.test.ts`. For import path manipulation, regex with `$` anchor.

## 2. Self-Import Preservation

**Test**: `tests/integration/self-import-preservation.test.ts`

**Scenario**: Barrel `index.ts` files that re-export from themselves:
```ts
// src/components/index.ts
export { Button } from './Button';
export { Card } from './Card';
```
Moving `src/components/` to `src/ui/` must:
- Move `index.ts` to `src/ui/index.ts`
- Update `./Button` → `./Button` (still same dir, no change needed)
- NOT create broken self-referential paths

**Key logic**: When source file and its imports are in the SAME directory being moved, relative paths between them don't change. The path resolver must recognize this.

## 3. Stack Overflow Protection

**Test**: `tests/integration/stack-overflow-protection.test.ts`

**Problem**: ts-morph (and any AST parser) can overflow the call stack on:
- Deeply nested JSX (>100 levels)
- Large object literals with many nested properties
- Very large files (>50k chars)

**Original fix**: The `safe-parser.service.ts` uses iterative BFS instead of recursive DFS. Has safeguards:
- maxDepth: 100
- timeoutMs: 5000
- skipComplexNodes: skips nodes with >10 nested `{}`, >20 JSX attributes, >50000 chars
- Fallback to top-level-only parsing on error

**Rust**: swc's parser is already highly optimized and non-recursive for parsing. The AST visitor (`swc_ecma_visit`) does use recursion, but Rust's stack is much larger than Node's. However, extremely large files should still be handled:
- Implement a simple depth check in visitor
- For truly massive files, fall back to regex import extraction (like `simple-import-updater.service.ts`)

## 4. Deep Directory Nesting

**Test**: `tests/integration/deeplyNestedMigration.test.ts`

**Scenario**: Moving files 4+ levels deep:
```
src/deep/path/to/module/file.ts
```
To:
```
src/flat/file.ts
```

**All imports must recalculate:**
- `import { x } from '../../../other'` → `import { x } from '../other'`
- `import { x } from './sibling'` → stays `./sibling`
- `import { x } from '../../../../../../../far/away'` → extreme relative paths

**Edge**: `../../../` going above project root should not happen — the destination is always inside the project. But need to handle gracefully if it does.

## 5. Directory Structure Preservation

**Test**: `tests/integration/advancedMoves.test.ts`

When moving a directory `src/old/` to `src/new/`, the internal directory structure must be preserved:
```
src/old/components/Button.ts      → src/new/old/components/Button.ts
src/old/hooks/useData.ts          → src/new/old/hooks/useData.ts
```

The source directory name (`old/`) is preserved under the destination. This is controlled by `sourceDirRoot = path.dirname(absSrc)` in [src/lib/move-files.service.ts:97]. The relative path `old/components/Button.ts` is used to reconstruct the destination.

**But**: when moving individual files (not directories), the filename is appended directly to destination — no structure preservation.

## 6. File Rename vs Directory Move Detection

**Test**: `tests/e2e/cli.test.ts`

At [src/lib/file-operations.service.ts:74-76]:
```ts
const destExt = path.extname(destination);
const isFileRename = destExt && ['.ts', '.tsx', '.js', '.jsx'].includes(destExt);
```

If destination has a TypeScript extension AND there's only one source file → this is a file **rename**, not a move to a directory. Example:
```
tsmv src/Button.ts src/NewButton.ts   # rename
tsmv src/Button.ts src/components/    # move to directory
```

## 7. Multiple Source Files

**Test**: `tests/integration/advancedMoves.test.ts`

`tsmv file1.ts file2.ts file3.ts dest/`

The arg parsing [src/index.ts:73-74] treats last arg as destination, rest as sources. All files are individually moved to the destination directory (filenames preserved).

## 8. Non-.ts Extensions

The `--extensions` flag allows custom extensions. Default is `.ts,.tsx`. When set to e.g. `.js,.jsx`, the tool works with JavaScript files too. Extension matching affects:
- File collection/discovery
- Import path resolution (`.js` extension stripping)
- Destination type detection

## 9. tsconfig Not Found Fallback

At [src/lib/move-files.service.ts:446-466], if no tsconfig is found or parsing fails, ts-morph is configured with hardcoded compiler options:
```ts
compilerOptions: {
    target: Latest, module: ESNext, moduleResolution: Node,
    jsx: Preserve, strict: false, skipLibCheck: true
}
```
And project files are manually discovered via glob rather than `addSourceFilesFromTsConfig()`.

**Rust**: swc doesn't need tsconfig for parsing — it handles TS syntax natively. tsconfig is only needed for path alias resolution (absolute imports converter).

## 10. Index File Resolution

Import `'./components'` resolves to `./components/index.ts` (or `.tsx`). The path resolver must try:
1. `./components.ts`
2. `./components.tsx`  
3. `./components/index.ts`
4. `./components/index.tsx`

Ref: [src/lib/syntax-aware-import-updater/import-resolver.service.ts:48-67] and [src/lib/simple-import-updater.service.ts:100-103].

## 11. Force Overwrite Behavior

When `--force` is set and destination exists:
- ts-morph: delete existing file from project, then move [src/lib/file-operations.service.ts:185-190]
- `file-handler.service.ts`: `fs.unlinkSync(destination)` then `fs.renameSync()` [src/lib/file-handler.service.ts:122-123]

**Rust**: `std::fs::rename` will error if destination exists. Must `std::fs::remove_file(destination)` first when force=true.

## 12. Cross-Project Moves (Monorepo)

From [src/commands/utils.ts:41-72]: `findAllTsConfigs()` recursively finds all tsconfig files in subdirectories, skipping `node_modules`. This supports monorepos where each package has its own tsconfig. The tool selects the closest tsconfig to the files being moved via `findTsConfigForFiles()` [src/commands/utils.ts:79-105].

**Rust**: Worth implementing for v1.1+. For v0.1, just support single tsconfig.
