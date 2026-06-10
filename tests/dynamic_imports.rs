//! End-to-end proof that the AST-based rewriter fixes the forms a regex matcher
//! used to miss: dynamic `import()`, CommonJS `require()`, and `jest.mock()`.
//! These run through the real `move_files` pipeline (inbound updates + the moved
//! file's own-import recalculation).

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

fn move_one(root: &Path, src: &str, dest: &str) {
    let sources = vec![root.join(src).to_string_lossy().to_string()];
    let dest = root.join(dest).to_string_lossy().to_string();
    tsmv::move_files(&sources, &dest, &opts()).unwrap();
}

/// The moved file's own dynamic `import('./sibling')` is recomputed for its new dir.
#[test]
fn dynamic_import_in_moved_file_is_recomputed() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    ts_project(root);

    write(&root.join("src/feature/sibling.ts"), "export const s = 1;\n");
    write(
        &root.join("src/feature/mover.ts"),
        "export const load = () => import('./sibling');\n",
    );

    move_one(root, "src/feature/mover.ts", "src/dest");

    let moved = fs::read_to_string(root.join("src/dest/mover.ts")).unwrap();
    assert!(
        moved.contains("import('../feature/sibling')"),
        "dynamic import should be recomputed. Got:\n{moved}"
    );
}

/// An external consumer's `require('../a/target')` is repointed to the new path.
#[test]
fn require_to_moved_file_is_updated() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    ts_project(root);

    write(&root.join("src/a/target.ts"), "module.exports = 1;\n");
    write(
        &root.join("src/b/consumer.ts"),
        "const t = require('../a/target');\nexports.t = t;\n",
    );

    move_one(root, "src/a/target.ts", "src/dest");

    let consumer = fs::read_to_string(root.join("src/b/consumer.ts")).unwrap();
    assert!(
        consumer.contains("require('../dest/target')"),
        "require() should be repointed. Got:\n{consumer}"
    );
}

/// A `jest.mock('../a/target')` call in a test file is repointed when the target moves.
#[test]
fn jest_mock_to_moved_file_is_updated() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    ts_project(root);

    write(&root.join("src/a/target.ts"), "export const t = 1;\n");
    write(
        &root.join("src/__tests__/target.test.ts"),
        "jest.mock('../a/target');\nimport { t } from '../a/target';\n",
    );

    move_one(root, "src/a/target.ts", "src/dest");

    let test = fs::read_to_string(root.join("src/__tests__/target.test.ts")).unwrap();
    assert!(
        test.contains("jest.mock('../dest/target')"),
        "jest.mock() should be repointed. Got:\n{test}"
    );
    assert!(
        test.contains("import { t } from '../dest/target'"),
        "static import should also be repointed. Got:\n{test}"
    );
}

/// Specifiers that only look like imports (in comments / string literals) must
/// not be rewritten — the regret a regex matcher could not avoid.
#[test]
fn lookalike_specifiers_in_comments_are_not_touched() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    ts_project(root);

    write(&root.join("src/a/target.ts"), "export const t = 1;\n");
    write(
        &root.join("src/b/consumer.ts"),
        "// import { t } from '../a/target';\n\
         const note = \"see import { t } from '../a/target'\";\n\
         import { t } from '../a/target';\n",
    );

    move_one(root, "src/a/target.ts", "src/dest");

    let consumer = fs::read_to_string(root.join("src/b/consumer.ts")).unwrap();
    assert!(
        consumer.contains("// import { t } from '../a/target';"),
        "comment must be untouched. Got:\n{consumer}"
    );
    assert!(
        consumer.contains("\"see import { t } from '../a/target'\""),
        "string literal must be untouched. Got:\n{consumer}"
    );
    assert!(
        consumer.contains("import { t } from '../dest/target';"),
        "the real import must be repointed. Got:\n{consumer}"
    );
}
