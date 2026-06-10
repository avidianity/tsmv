# tsmv — LLM Usage Reference

PURPOSE: Move/rename TypeScript/JavaScript files and folders, then rewrite
affected module references so the project still compiles.
MECHANISM: AST-based — parses each file with the tree-sitter TypeScript/TSX
grammar and rewrites real specifier nodes (no regex; no false matches inside
comments or strings).
This document is optimized for programmatic/agent use. Prefer constructing exact
commands from the rules below over guessing.

## INVOCATION GRAMMAR

```
tsmv [OPTIONS] <SOURCE>... <DEST>          # default form
tsmv move [OPTIONS] <SOURCE>... <DEST>     # explicit subcommand, identical
tsmv generate-completions <SHELL>          # SHELL ∈ {bash,zsh,fish,elvish,powershell}
tsmv self-update [--force]                 # alias: update — replace binary with latest release (Linux/macOS)
tsmv self-uninstall [-y|--yes]             # alias: uninstall — delete the installed binary (Linux/macOS)
tsmv --usage-llm                           # print THIS document, exit 0
tsmv --help | tsmv --version
```

RULE: The LAST positional argument is ALWAYS `<DEST>`. All earlier positionals are
sources. Minimum 2 positionals (≥1 source + 1 dest). Fewer → exit 1.

## FLAGS (name | default | type | effect)

```
-r, --recursive        | off    | bool | recurse into directory sources
-i, --interactive      | off    | bool | prompt before overwriting an existing dest file
-f, --force            | off    | bool | overwrite existing dest without prompting
-n, --dry-run          | off    | bool | print planned moves to stdout, change nothing
-v, --verbose          | off    | bool | detailed logs to STDERR
--extensions <CSV>     | .ts,.tsx | str | comma list; leading dot optional ("ts"≡".ts")
--tsconfig <PATH>      | auto   | path | tsconfig.json; if omitted, search upward from sources
--absolute-imports     | ON     | bool | convert relative→absolute alias imports (needs tsconfig)
--no-absolute-imports  | —      | flag | disable absolute conversion; keep imports relative
--alias-prefix <STR>   | @      | str  | prefix for generated absolute imports
--usage-llm            | —      | flag | print this doc, exit
-h, --help / -V, --version
```

All flags are global (valid before or after a subcommand).

## SEMANTIC INVARIANTS

- Single source whose DEST ends in `.ts|.tsx|.js|.jsx` ⇒ RENAME (dest is the new file path).
- Otherwise DEST is treated as a directory; sources are placed inside it by filename.
- Directory source ⇒ internal structure is preserved under DEST.
- Inbound rewrite: every other project file importing a moved file is repointed to the new path.
- Self rewrite: relative imports INSIDE a moved file are recomputed for the new dir.
  - Files moved together keep their mutual imports relative/internal.
  - Imports to files left behind are repointed (e.g. `./sib` → `../old/sib`).
- Empty source directories are removed after the move.
- Circular dependency among moved files ⇒ advisory STDERR warning only; move still succeeds.
- `--absolute-imports` (default) + tsconfig found ⇒ rewrites relative imports across the
  ENTIRE project (every .ts/.tsx/.js/.jsx, skipping node_modules,dist,build,.next,.git,target),
  using tsconfig `baseUrl`/`paths`, falling back to mapping `src/` → `<alias-prefix>`.
- `--absolute-imports` with NO tsconfig found ⇒ conversion silently skipped.
- Bare specifiers (`react`, `lodash`) and already-absolute imports are never modified.

## REWRITTEN SPECIFIER FORMS (multi-line tolerated; literal string path required)

```
import x from '…'            import { a, b } from '…'      import * as ns from '…'
import type { T } from '…'   import '…'  (side-effect)
export { a } from '…'        export * from '…'             export * as ns from '…'
export type { T } from '…'
import('…')   (dynamic)      require('…')  (CommonJS)
jest.mock('…') | vi.mock('…') (and jest/vi loader siblings)
```

## NOT REWRITTEN (unresolvable statically — by design, true of any tool)

- Specifier stored in a variable:        `import(modulePath)` / `require(name)`
- Interpolated template literal:         `` import(`./x/${y}`) ``
- Specifiers in comments or string literals are correctly IGNORED (not a miss).

## IO / EXIT

- STDOUT: dry-run plan, completion scripts, `--usage-llm`, `--help`, `--version`.
- STDERR: verbose logs, warnings, errors.
- EXIT 0: success. EXIT 1: missing source/dest, unsupported shell, or move failure.

## DECISION RULES (agent guidance)

- Unsure of impact ⇒ run with `--dry-run` first; parse stdout lines `SRC → DEST`.
- Want a pure file move with imports UNTOUCHED in style ⇒ add `--no-absolute-imports`.
- Moving a folder ⇒ you MUST pass `-r`, else directory sources are not expanded.
- Overwriting may be required ⇒ add `-f` (non-interactive) or `-i` (prompt). Without
  either, an existing destination causes that file to be skipped with an error.
- Need absolute imports but alias differs from `@` ⇒ set `--alias-prefix`.
- Non-default extensions (e.g. include `.js`) ⇒ set `--extensions`.

## COMMAND → OUTCOME EXAMPLES

```
tsmv src/Button.tsx src/components/
  → moves file into dir; repoints all imports of Button; converts project to absolute imports.

tsmv src/utils.ts src/helpers.ts
  → RENAME (dest has .ts ext); updates importers of utils.

tsmv -r src/legacy src/modern/
  → recursive dir move preserving structure under src/modern/legacy/…

tsmv --dry-run -r src/legacy src/modern/
  → prints planned "SRC → DEST" lines to stdout; no filesystem change; exit 0.

tsmv --no-absolute-imports src/a.ts src/b.ts src/lib/
  → moves both files into src/lib/; fixes imports; leaves import style relative.

tsmv --alias-prefix "~" --tsconfig ./tsconfig.json src/Button.tsx src/ui/
  → absolute imports generated with "~" prefix using the given tsconfig.

tsmv generate-completions zsh > ~/.zfunc/_tsmv
  → writes a zsh completion script to stdout.
```
