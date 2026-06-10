# tsmv

[![CI](https://github.com/avidianity/tsmv/actions/workflows/ci.yml/badge.svg)](https://github.com/avidianity/tsmv/actions/workflows/ci.yml)

**Safely move TypeScript files and folders — and keep your imports working.**

```bash
tsmv src/Button.tsx src/components/
```

Every file that imported `Button` is repointed to its new location, the moved
file's own relative imports are recomputed, and (by default) relative imports are
converted to absolute alias imports using your `tsconfig.json`.

---

## Features

- **Move, rename, and reorganize** single files, multiple files, or whole
  directories (`-r`).
- **Updates inbound imports** — every importer of a moved file is rewritten.
- **Recomputes the moved file's own imports** for its new location.
- **Real parser, not regex** — handles static imports/exports, side-effect
  imports, dynamic `import()`, `require()`, and `jest`/`vi` mocks, and never
  touches import-looking text in comments or strings.
- **Absolute-import conversion** driven by `tsconfig.json` `baseUrl`/`paths`
  (toggle with `--no-absolute-imports`).
- **Dry-run** (`-n`) to preview every change before touching disk.
- **Circular-dependency warnings** and empty-directory cleanup.
- **Shell completions** for bash, zsh, fish, elvish, and PowerShell.
- Single static binary, no Node.js runtime required.

---

## Installation

### Quick install (Linux & macOS)

```sh
sh -c "$(curl -fsSL https://raw.githubusercontent.com/avidianity/tsmv/master/install.sh)"
```

This downloads the right prebuilt binary for your platform, verifies its
SHA-256 checksum, and installs it to `~/.local/bin`. Override the location with
`TSMV_INSTALL_DIR`, or pin a version with `TSMV_VERSION=v1.0.0`.

### From source

```bash
cargo install --path .
# or build a release binary:
cargo build --release   # → target/release/tsmv
```

### Prebuilt binaries

Prefer to do it by hand? Download the archive for your platform from the
[Releases](https://github.com/avidianity/tsmv/releases) page (Linux, macOS, and
Windows, with SHA-256 checksums), unpack it, and put `tsmv` on your `PATH`.

### Updating & uninstalling

```bash
tsmv self-update       # replace the running binary with the latest release
tsmv self-uninstall    # remove the installed binary (asks first; -y to skip)
```

---

## Quick start

```bash
# Move a file into a folder and fix all imports
tsmv src/Button.tsx src/components/

# Rename a file (destination ends in a TS/JS extension)
tsmv src/utils.ts src/helpers.ts

# Move a directory recursively, preserving its structure
tsmv -r src/legacy src/modern/

# Preview the plan without changing anything
tsmv --dry-run -r src/legacy src/modern/

# Move but keep imports relative
tsmv --no-absolute-imports src/Button.tsx src/components/
```

The last argument is always the destination; everything before it is a source.

---

## Documentation

- **[usage.md](./usage.md)** — full usage guide: every flag, behaviour, and
  example.
- **[usage.llm.md](./usage.llm.md)** — the same reference, condensed and
  structured for LLM/agent consumption. Print it straight from the binary:

  ```bash
  tsmv --usage-llm
  ```

---

## How it works

1. Discover the source files (expanding directories with `-r`, filtered by
   `--extensions`).
2. Plan destinations — directory moves preserve structure; a single source onto a
   `.ts`/`.tsx`/`.js`/`.jsx` path is a rename.
3. Move the files on disk.
4. Rewrite inbound imports across the project, then recompute the moved files'
   own relative imports.
5. Optionally convert relative imports to absolute alias imports via `tsconfig`.
6. Clean up emptied directories and warn on circular dependencies.

Module references are found by parsing each file with a real TypeScript/TSX
grammar ([tree-sitter](https://tree-sitter.github.io/)) — not regular
expressions. That means look-alike text in comments and strings is never
touched, and dynamic `import()`, `require()`, and `jest`/`vi` mock calls are
rewritten just like static imports. The only specifiers left alone are those
whose path is computed at runtime (a variable or interpolated template literal);
see [Notes](./usage.md#notes).

---

## License

[GNU Affero General Public License v3.0](./LICENSE) (AGPL-3.0-only).
