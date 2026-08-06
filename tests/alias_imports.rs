//! Moving files in a project that imports through tsconfig `paths` aliases.
//!
//! Regression coverage for a bug where the import updater only ever considered
//! specifiers starting with `.`, so a codebase that imports exclusively through
//! aliases (`@/components/shell`) had its files moved but not one import
//! rewritten, leaving the project unable to typecheck.

use std::fs;
use std::path::Path;

use tempfile::TempDir;

fn write(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

/// An alias-only project: `@/*` maps to `./src/*`, and nothing imports
/// relatively.
fn setup_alias_project(dir: &Path) {
    write(
        &dir.join("tsconfig.json"),
        r#"{"compilerOptions":{"baseUrl":".","paths":{"@/*":["./src/*"]}}}"#,
    );
    write(
        &dir.join("src/components/shell.tsx"),
        "export function Shell() { return null; }\n",
    );
    write(
        &dir.join("src/shared/api/queries.ts"),
        "export const queries = {};\n",
    );
    write(
        &dir.join("src/pages/ai-assistant.tsx"),
        "import { Shell } from '@/components/shell';\n\
         import { queries } from '@/shared/api/queries';\n\
         export function AiAssistantPage() { return Shell(); }\n",
    );
    write(
        &dir.join("src/app/router.tsx"),
        "import { Shell } from '@/components/shell';\n\
         import { AiAssistantPage } from '@/pages/ai-assistant';\n\
         export const router = [Shell, AiAssistantPage];\n",
    );
}

fn tsmv(dir: &Path, args: &[&str]) -> assert_cmd::assert::Assert {
    let mut cmd = assert_cmd::Command::cargo_bin("tsmv").unwrap();
    cmd.args(args).current_dir(dir);
    cmd.assert()
}

#[test]
fn importers_using_an_alias_are_repointed_at_the_new_path() {
    let dir = TempDir::new().unwrap();
    setup_alias_project(dir.path());

    tsmv(
        dir.path(),
        &[
            "--tsconfig",
            "tsconfig.json",
            "src/pages/ai-assistant.tsx",
            "src/features/ai-assistant/pages/assistant.tsx",
        ],
    )
    .success();

    let router = fs::read_to_string(dir.path().join("src/app/router.tsx")).unwrap();
    assert!(
        router.contains("'@/features/ai-assistant/pages/assistant'"),
        "the alias import should follow the move. Got:\n{router}"
    );
    assert!(
        !router.contains("'@/pages/ai-assistant'"),
        "the old alias should be gone. Got:\n{router}"
    );
}

#[test]
fn a_moved_files_aliases_to_files_that_did_not_move_are_left_alone() {
    let dir = TempDir::new().unwrap();
    setup_alias_project(dir.path());

    tsmv(
        dir.path(),
        &[
            "--tsconfig",
            "tsconfig.json",
            "src/pages/ai-assistant.tsx",
            "src/features/ai-assistant/pages/assistant.tsx",
        ],
    )
    .success();

    // An alias names a fixed path, so moving the importing file changes nothing.
    let moved =
        fs::read_to_string(dir.path().join("src/features/ai-assistant/pages/assistant.tsx"))
            .unwrap();
    assert!(
        moved.contains("'@/components/shell'"),
        "a still-valid alias must not be rewritten. Got:\n{moved}"
    );
    assert!(
        moved.contains("'@/shared/api/queries'"),
        "a still-valid alias must not be rewritten. Got:\n{moved}"
    );
}

#[test]
fn aliases_between_two_files_moved_together_are_updated() {
    let dir = TempDir::new().unwrap();
    write(
        &dir.path().join("tsconfig.json"),
        r#"{"compilerOptions":{"baseUrl":".","paths":{"@/*":["./src/*"]}}}"#,
    );
    write(&dir.path().join("src/api/live.ts"), "export const live = 1;\n");
    write(
        &dir.path().join("src/api/queries.ts"),
        "import { live } from '@/api/live';\nexport const queries = live;\n",
    );

    tsmv(
        dir.path(),
        &[
            "--tsconfig",
            "tsconfig.json",
            "-r",
            "src/api",
            "src/features/ai/",
        ],
    )
    .success();

    let queries =
        fs::read_to_string(dir.path().join("src/features/ai/api/queries.ts")).unwrap();
    assert!(
        queries.contains("'@/features/ai/api/live'"),
        "an alias to a file that moved alongside it should be updated. Got:\n{queries}"
    );
}

#[test]
fn a_sequence_of_renames_converges_on_a_consistent_tree() {
    // Mirrors a feature-folder reorg performed as several separate commands.
    let dir = TempDir::new().unwrap();
    setup_alias_project(dir.path());

    let moves = [
        (
            "src/pages/ai-assistant.tsx",
            "src/features/ai-assistant/pages/assistant.tsx",
        ),
        ("src/shared/api/queries.ts", "src/features/ai-assistant/api/queries.ts"),
        ("src/components/shell.tsx", "src/shell/components/shell.tsx"),
    ];
    for (from, to) in moves {
        tsmv(dir.path(), &["--tsconfig", "tsconfig.json", from, to]).success();
    }

    let router = fs::read_to_string(dir.path().join("src/app/router.tsx")).unwrap();
    assert!(router.contains("'@/shell/components/shell'"), "got:\n{router}");
    assert!(
        router.contains("'@/features/ai-assistant/pages/assistant'"),
        "got:\n{router}"
    );

    let moved =
        fs::read_to_string(dir.path().join("src/features/ai-assistant/pages/assistant.tsx"))
            .unwrap();
    assert!(moved.contains("'@/shell/components/shell'"), "got:\n{moved}");
    assert!(
        moved.contains("'@/features/ai-assistant/api/queries'"),
        "got:\n{moved}"
    );
}

#[test]
fn bare_package_specifiers_are_never_touched() {
    let dir = TempDir::new().unwrap();
    write(
        &dir.path().join("tsconfig.json"),
        r#"{"compilerOptions":{"baseUrl":".","paths":{"@/*":["./src/*"]}}}"#,
    );
    write(&dir.path().join("src/widgets/a.ts"), "export const a = 1;\n");
    write(
        &dir.path().join("src/app/main.ts"),
        "import React from 'react';\n\
         import { debounce } from 'lodash/debounce';\n\
         import { a } from '@/widgets/a';\n\
         export const main = [React, debounce, a];\n",
    );

    tsmv(
        dir.path(),
        &["--tsconfig", "tsconfig.json", "src/widgets/a.ts", "src/ui/a.ts"],
    )
    .success();

    let main = fs::read_to_string(dir.path().join("src/app/main.ts")).unwrap();
    assert!(main.contains("from 'react'"), "got:\n{main}");
    assert!(main.contains("from 'lodash/debounce'"), "got:\n{main}");
    assert!(main.contains("'@/ui/a'"), "got:\n{main}");
}

#[test]
fn each_import_keeps_the_form_it_was_written_in() {
    let dir = TempDir::new().unwrap();
    write(
        &dir.path().join("tsconfig.json"),
        r#"{"compilerOptions":{"baseUrl":".","paths":{"@/*":["./src/*"]}}}"#,
    );
    write(&dir.path().join("src/widgets/b.ts"), "export const b = 2;\n");
    write(
        &dir.path().join("src/widgets/a.ts"),
        "export const a = 1;\n",
    );
    write(
        &dir.path().join("src/app/main.ts"),
        "import { a } from '@/widgets/a';\n\
         import { b } from '../widgets/b';\n\
         export const main = [a, b];\n",
    );

    // --no-absolute-imports so the alias pass cannot mask the updater's choice.
    tsmv(
        dir.path(),
        &[
            "--tsconfig",
            "tsconfig.json",
            "--no-absolute-imports",
            "src/widgets/a.ts",
            "src/ui/a.ts",
        ],
    )
    .success();

    let main = fs::read_to_string(dir.path().join("src/app/main.ts")).unwrap();
    assert!(
        main.contains("'@/ui/a'"),
        "an alias import must stay an alias. Got:\n{main}"
    );
    assert!(
        main.contains("'../widgets/b'"),
        "an untouched relative import must stay relative. Got:\n{main}"
    );
}

#[test]
fn a_relative_import_inside_a_moved_file_is_still_recomputed() {
    let dir = TempDir::new().unwrap();
    write(
        &dir.path().join("tsconfig.json"),
        r#"{"compilerOptions":{"baseUrl":".","paths":{"@/*":["./src/*"]}}}"#,
    );
    write(&dir.path().join("src/widgets/b.ts"), "export const b = 2;\n");
    write(
        &dir.path().join("src/widgets/a.ts"),
        "import { b } from './b';\nexport const a = b;\n",
    );

    tsmv(
        dir.path(),
        &[
            "--tsconfig",
            "tsconfig.json",
            "--no-absolute-imports",
            "src/widgets/a.ts",
            "src/ui/a.ts",
        ],
    )
    .success();

    let moved = fs::read_to_string(dir.path().join("src/ui/a.ts")).unwrap();
    assert!(
        moved.contains("'../widgets/b'"),
        "a relative import must be recomputed from the new location. Got:\n{moved}"
    );
}

#[test]
fn the_most_specific_alias_root_is_used() {
    let dir = TempDir::new().unwrap();
    write(
        &dir.path().join("tsconfig.json"),
        r#"{"compilerOptions":{"baseUrl":"./src","paths":{"@/*":["*"],"@lib/*":["lib/*"]}}}"#,
    );
    write(&dir.path().join("src/lib/util.ts"), "export const util = 1;\n");
    write(
        &dir.path().join("src/app/main.ts"),
        "import { util } from '@lib/util';\nexport const main = util;\n",
    );

    tsmv(
        dir.path(),
        &[
            "--tsconfig",
            "tsconfig.json",
            "src/lib/util.ts",
            "src/lib/helpers/util.ts",
        ],
    )
    .success();

    let main = fs::read_to_string(dir.path().join("src/app/main.ts")).unwrap();
    assert!(
        main.contains("'@lib/helpers/util'"),
        "the @lib root is more specific than @/. Got:\n{main}"
    );
}

#[test]
fn a_move_outside_every_alias_root_falls_back_to_a_relative_import() {
    let dir = TempDir::new().unwrap();
    write(
        &dir.path().join("tsconfig.json"),
        r#"{"compilerOptions":{"baseUrl":".","paths":{"@/*":["./src/*"]}}}"#,
    );
    write(&dir.path().join("src/app/thing.ts"), "export const thing = 1;\n");
    write(
        &dir.path().join("src/app/main.ts"),
        "import { thing } from '@/app/thing';\nexport const main = thing;\n",
    );

    tsmv(
        dir.path(),
        &[
            "--tsconfig",
            "tsconfig.json",
            "src/app/thing.ts",
            "outside/thing.ts",
        ],
    )
    .success();

    let main = fs::read_to_string(dir.path().join("src/app/main.ts")).unwrap();
    assert!(
        main.contains("../../outside/thing"),
        "no alias covers the new location, so it must fall back to a relative \
         path rather than emit an alias the compiler cannot resolve. Got:\n{main}"
    );
    assert!(!main.contains("'@/app/thing'"), "got:\n{main}");
}

#[test]
fn quote_style_is_preserved() {
    let dir = TempDir::new().unwrap();
    write(
        &dir.path().join("tsconfig.json"),
        r#"{"compilerOptions":{"baseUrl":".","paths":{"@/*":["./src/*"]}}}"#,
    );
    write(&dir.path().join("src/widgets/a.ts"), "export const a = 1;\n");
    write(
        &dir.path().join("src/app/single.ts"),
        "import { a } from '@/widgets/a';\nexport const s = a;\n",
    );
    write(
        &dir.path().join("src/app/double.ts"),
        "import { a } from \"@/widgets/a\";\nexport const d = a;\n",
    );

    tsmv(
        dir.path(),
        &["--tsconfig", "tsconfig.json", "src/widgets/a.ts", "src/ui/a.ts"],
    )
    .success();

    let single = fs::read_to_string(dir.path().join("src/app/single.ts")).unwrap();
    let double = fs::read_to_string(dir.path().join("src/app/double.ts")).unwrap();
    assert!(single.contains("'@/ui/a'"), "got:\n{single}");
    assert!(double.contains("\"@/ui/a\""), "got:\n{double}");
}
