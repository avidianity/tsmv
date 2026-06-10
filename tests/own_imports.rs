//! Regression tests for the "moved file's own imports" bug: when a file is moved,
//! its own relative imports must be recomputed for its new location — not just the
//! imports that point *to* it.

use std::fs;
use std::path::Path;

use tempfile::TempDir;
use tsmv::options::MoveOptions;

fn opts() -> MoveOptions {
    MoveOptions {
        force: true,
        recursive: true,
        absolute_imports: false,
        ..MoveOptions::default()
    }
}

fn write(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn ts_project(dir: &Path) {
    fs::write(
        dir.join("tsconfig.json"),
        r#"{"compilerOptions":{"target":"es2020"},"include":["src/**/*"]}"#,
    )
    .unwrap();
}

/// Scenario A: move one file out, leaving its imported sibling behind.
/// The moved file's `./sibling` import must become `../feature/sibling`.
#[test]
fn moved_file_import_to_non_moved_sibling_is_recomputed() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    ts_project(root);

    write(&root.join("src/feature/sibling.ts"), "export const sib = 1;\n");
    write(
        &root.join("src/feature/mover.ts"),
        "import { sib } from './sibling';\nexport const m = sib + 1;\n",
    );

    let sources = vec![root.join("src/feature/mover.ts").to_string_lossy().to_string()];
    let dest = root.join("src/dest").to_string_lossy().to_string();
    tsmv::move_files(&sources, &dest, &opts()).unwrap();

    let moved = fs::read_to_string(root.join("src/dest/mover.ts")).unwrap();
    assert!(
        moved.contains("from '../feature/sibling'"),
        "moved file's own import should be recomputed. Got:\n{moved}"
    );
    assert!(root.join("src/feature/sibling.ts").exists(), "sibling stays put");
}

/// Scenario B: move two interdependent files together to one destination.
/// Their mutual import (`./b`) must stay relative-internal and unchanged.
#[test]
fn two_files_moved_together_keep_internal_import() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    ts_project(root);

    write(&root.join("src/old/b.ts"), "export const b = 1;\n");
    write(
        &root.join("src/old/a.ts"),
        "import { b } from './b';\nexport const a = b + 1;\n",
    );

    let sources = vec![
        root.join("src/old/a.ts").to_string_lossy().to_string(),
        root.join("src/old/b.ts").to_string_lossy().to_string(),
    ];
    let dest = root.join("src/new").to_string_lossy().to_string();
    tsmv::move_files(&sources, &dest, &opts()).unwrap();

    let a = fs::read_to_string(root.join("src/new/a.ts")).unwrap();
    assert!(
        a.contains("from './b'"),
        "internal import between co-moved files should stay './b'. Got:\n{a}"
    );
}

/// Scenario C: recursive directory move where an internal file imports both an
/// internal sibling (stays relative-internal) and an external file (recomputed).
#[test]
fn directory_move_recomputes_external_but_keeps_internal() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    ts_project(root);

    write(&root.join("src/keep/external.ts"), "export const k = 9;\n");
    write(&root.join("src/lib/helper.ts"), "export const h = 1;\n");
    write(
        &root.join("src/lib/main.ts"),
        "import { h } from './helper';\nimport { k } from '../keep/external';\nexport const x = h + k;\n",
    );

    let sources = vec![root.join("src/lib").to_string_lossy().to_string()];
    let dest = root.join("src/dest").to_string_lossy().to_string();
    tsmv::move_files(&sources, &dest, &opts()).unwrap();

    let main = fs::read_to_string(root.join("src/dest/lib/main.ts")).unwrap();
    assert!(
        main.contains("from './helper'"),
        "internal import should stay './helper'. Got:\n{main}"
    );
    assert!(
        main.contains("from '../../keep/external'"),
        "external import should be recomputed to '../../keep/external'. Got:\n{main}"
    );
}

/// Scenario D: an external consumer pointing *to* the moved file is still updated
/// (guards against the recompute pass breaking the original inbound-update path).
#[test]
fn external_consumer_pointing_to_moved_file_is_updated() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    ts_project(root);

    write(&root.join("src/a/target.ts"), "export const t = 1;\n");
    write(
        &root.join("src/b/consumer.ts"),
        "import { t } from '../a/target';\nexport const u = t;\n",
    );

    let sources = vec![root.join("src/a/target.ts").to_string_lossy().to_string()];
    let dest = root.join("src/dest").to_string_lossy().to_string();
    tsmv::move_files(&sources, &dest, &opts()).unwrap();

    let consumer = fs::read_to_string(root.join("src/b/consumer.ts")).unwrap();
    assert!(
        consumer.contains("from '../dest/target'"),
        "inbound import should be updated to '../dest/target'. Got:\n{consumer}"
    );
}

/// Multi-line named imports must be rewritten too (regex newline handling).
#[test]
fn multiline_import_is_recomputed() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    ts_project(root);

    write(&root.join("src/feature/sibling.ts"), "export const a = 1;\nexport const b = 2;\n");
    write(
        &root.join("src/feature/mover.ts"),
        "import {\n  a,\n  b,\n} from './sibling';\nexport const m = a + b;\n",
    );

    let sources = vec![root.join("src/feature/mover.ts").to_string_lossy().to_string()];
    let dest = root.join("src/dest").to_string_lossy().to_string();
    tsmv::move_files(&sources, &dest, &opts()).unwrap();

    let moved = fs::read_to_string(root.join("src/dest/mover.ts")).unwrap();
    assert!(
        moved.contains("from '../feature/sibling'"),
        "multi-line import should be recomputed. Got:\n{moved}"
    );
}
