# tsmv — Usage Guide

`tsmv` safely moves or renames TypeScript/JavaScript files and folders and
rewrites the affected `import`/`export` statements so your project keeps
compiling.

`tsmv` parses each file with a real TypeScript/TSX grammar
([tree-sitter](https://tree-sitter.github.io/)) and rewrites the actual module
references — so it never mistakes import-looking text inside a comment or a
string literal for a real import.

---

## Synopsis

```
tsmv [OPTIONS] <SOURCE>... <DEST>
tsmv move [OPTIONS] <SOURCE>... <DEST>
tsmv generate-completions <SHELL>
tsmv self-update [--force]
tsmv self-uninstall [--yes]
tsmv --usage-llm
tsmv --help
tsmv --version
```

The last positional argument is always the **destination**; everything before it
is treated as a **source**. You need at least one source and one destination.

---

## Commands

| Command | Description |
| --- | --- |
| *(default)* | Move sources to destination. `tsmv a.ts b.ts dir/` is shorthand for `tsmv move ...`. |
| `move` | Explicit move subcommand. Identical behaviour to the default form. |
| `generate-completions <SHELL>` | Print a shell completion script to stdout. `SHELL` is one of `bash`, `zsh`, `fish`, `elvish`, `powershell`. |
| `self-update` (alias `update`) | Download the latest release and replace the running binary in place. `--force` reinstalls even if already current. (Linux/macOS.) |
| `self-uninstall` (alias `uninstall`) | Remove the installed `tsmv` binary. Prompts for confirmation; pass `-y`/`--yes` to skip it. (Linux/macOS.) |

---

## Options

All options are global — they work before or after the subcommand.

| Flag | Default | Description |
| --- | --- | --- |
| `-r`, `--recursive` | off | Recurse into directories when a source is a folder. |
| `-i`, `--interactive` | off | Prompt for confirmation before overwriting an existing destination file. |
| `-f`, `--force` | off | Overwrite existing destination files without prompting. |
| `-n`, `--dry-run` | off | Print the planned moves and exit without touching the filesystem. |
| `-v`, `--verbose` | off | Print detailed operation logs to **stderr**. |
| `--extensions <CSV>` | `.ts,.tsx` | Comma-separated list of extensions to treat as source files. A leading dot is optional (`ts,tsx` and `.ts,.tsx` are equivalent). |
| `--tsconfig <PATH>` | auto-detect | Path to `tsconfig.json`. If omitted, `tsmv` searches upward from the source files. |
| `--absolute-imports` | **on** | After moving, rewrite relative imports to absolute alias imports (requires a tsconfig). |
| `--no-absolute-imports` | — | Disable absolute-import conversion; keep imports relative. |
| `--alias-prefix <PREFIX>` | `@` | Alias prefix used when generating absolute imports. |
| `--usage-llm` | — | Print the LLM-optimised usage guide (`usage.llm.md`) and exit. |
| `-h`, `--help` | — | Print help. |
| `-V`, `--version` | — | Print version. |

---

## What it does

When you move one or more files, `tsmv` performs the following steps:

1. **Discovers** the source files (expanding directories when `--recursive` is set,
   filtered by `--extensions`).
2. **Plans** the destination paths. Directory moves preserve their internal
   structure; a single source moved onto a path ending in a TS/JS extension is
   treated as a **rename**.
3. **Moves** the files on disk (honouring `--force` / `--interactive` for
   overwrites).
4. **Updates inbound imports**: every other file in the project that imported a
   moved file has its specifier rewritten to the new location.
   Both relative imports and tsconfig alias imports (`@/components/shell`) are
   followed, and each is rewritten in the form it was written in, so an
   alias-only codebase does not acquire relative paths.
5. **Recalculates the moved files' own imports**: relative imports *inside* a
   moved file are recomputed for its new directory. Imports between files moved
   together stay relative and internal; imports to files left behind are
   re-pointed.
   An alias import names a fixed path, so it is left alone unless the file it
   names also moved.
6. **Converts to absolute imports** (when `--absolute-imports` is on and a
   tsconfig is found) — see below.
7. **Cleans up** source directories that became empty.
8. **Warns** if it detects a circular dependency among the moved files (advisory
   only; the move still completes).

`tsmv` understands every module-reference form, including statements split
across multiple lines:

```ts
import x from './m';
import { a, b } from './m';
import * as ns from './m';
import type { T } from './m';
import './m';                       // side-effect import
export { a } from './m';
export * from './m';
export * as ns from './m';
export type { T } from './m';
const m = await import('./m');      // dynamic import
const r = require('./m');           // CommonJS require
jest.mock('./m');                   // jest / vi mock + loader calls
```

---

## Absolute imports

By default (`--absolute-imports` is on), once the move completes and a
`tsconfig.json` is located, `tsmv` rewrites **relative imports across the whole
project** into absolute alias imports.

- It reads `compilerOptions.baseUrl` and `compilerOptions.paths` from the tsconfig
  to decide how to map paths.
  Each `paths` target is resolved against `baseUrl`, exactly as TypeScript does, so
  `baseUrl: "./src"` with `"@/*": ["*"]` and `baseUrl: "."` with `"@/*": ["./src/*"]`
  both produce `@/components/panel`.
  A `baseUrl` inherited through `extends` stays anchored to the config file that
  declared it.
  When `paths` is set without a `baseUrl`, targets resolve against the directory of
  the tsconfig that declared them, matching TypeScript 4.1 and later.
- When several aliases match, the most specific one wins.
- An import that no alias covers is left relative, since an alias the compiler
  cannot resolve would break the build.
  If the tsconfig defines no `paths` at all, `tsmv` falls back to mapping the
  `baseUrl` directory to the `--alias-prefix` (default `@`).
- Files under `node_modules`, `dist`, `build`, `.next`, `.git`, and `target` are
  skipped.
- Bare package specifiers (`react`, `lodash`, …) and imports already written in
  alias form are left untouched by this pass.

> **Note:** this conversion touches every TypeScript/JavaScript file in the
> project, not only the files you moved. If you want imports to stay relative,
> pass `--no-absolute-imports`. If no tsconfig is found, the conversion is
> silently skipped.

---

## Examples

Move a single file into a folder and fix all imports:

```bash
tsmv src/Button.tsx src/components/
```

Rename a file (destination ends in a TS/JS extension):

```bash
tsmv src/utils.ts src/helpers.ts
```

Move several files at once:

```bash
tsmv src/a.ts src/b.ts src/lib/
```

Move a directory recursively, keeping its structure:

```bash
tsmv -r src/legacy src/modern/
```

Preview without making changes:

```bash
tsmv --dry-run -r src/legacy src/modern/
```

Move but keep imports relative:

```bash
tsmv --no-absolute-imports src/Button.tsx src/components/
```

Use a custom alias prefix and an explicit tsconfig:

```bash
tsmv --alias-prefix "~" --tsconfig ./tsconfig.json src/Button.tsx src/ui/
```

Restrict which extensions are treated as source files:

```bash
tsmv --extensions ts,tsx,js -r src/old src/new/
```

Install shell completions (zsh example):

```bash
tsmv generate-completions zsh > ~/.zfunc/_tsmv
```

---

## Exit codes

| Code | Meaning |
| --- | --- |
| `0` | Success. |
| `1` | Usage error (missing source/destination), an unsupported shell, or a move failure. |

Verbose and error diagnostics are written to **stderr**; the dry-run plan and
shell-completion scripts are written to **stdout**.

---

## Notes

`tsmv` resolves and rewrites every **statically written** module specifier
(see the list above). The only specifiers it leaves alone are ones whose value
isn't known until runtime, because no tool can resolve them statically:

- A path stored in a variable — `import(modulePath)`, `require(name)`.
- An interpolated template literal — `` import(`./locales/${lang}`) ``.

These are skipped deliberately rather than rewritten incorrectly. Everything with
a literal string path is handled. If you ever want to eyeball the result first,
run with `--dry-run`.
