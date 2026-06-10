use std::fs;

use tempfile::TempDir;

use tsmv::options::MoveOptions;

fn setup_project(dir: &TempDir) {
    let src = dir.path().join("src");
    let old = src.join("old");
    let new = src.join("new");
    let consumer = src.join("consumer");
    fs::create_dir_all(&old).unwrap();
    fs::create_dir_all(&new).unwrap();
    fs::create_dir_all(&consumer).unwrap();

    // File to be moved
    fs::write(
        old.join("target.ts"),
        "export const x = 1;\nexport type T = number;\n",
    )
    .unwrap();

    fs::write(
        dir.path().join("tsconfig.json"),
        r#"{"compilerOptions":{"target":"es2020"},"include":["src/**/*"]}"#,
    )
    .unwrap();
}

#[test]
fn test_import_type() {
    let dir = TempDir::new().unwrap();
    setup_project(&dir);

    let src = dir.path().join("src");
    let consumer = src.join("consumer");

    // import type { ... } from (type before braces)
    fs::write(
        consumer.join("user.ts"),
        "import type { T } from '../old/target';\nexport const y: T = 1;\n",
    )
    .unwrap();

    let options = MoveOptions {
        force: true,
        verbose: false,
        absolute_imports: false,
        ..MoveOptions::default()
    };

    tsmv::move_files(
        &[src.join("old").join("target.ts").to_string_lossy().to_string()],
        src.join("new").to_string_lossy().as_ref(),
        &options,
    )
    .unwrap();

    let content = fs::read_to_string(consumer.join("user.ts")).unwrap();
    assert!(
        content.contains("import type { T } from '../new/target'"),
        "import type should update. Got:\n{content}"
    );
}

#[test]
fn test_side_effect_import() {
    let dir = TempDir::new().unwrap();
    setup_project(&dir);

    let src = dir.path().join("src");
    let consumer = src.join("consumer");

    // Side-effect import (no from, just import '...')
    fs::write(
        consumer.join("init.ts"),
        "import '../old/target';\nexport const initialized = true;\n",
    )
    .unwrap();

    let options = MoveOptions {
        force: true,
        verbose: false,
        absolute_imports: false,
        ..MoveOptions::default()
    };

    tsmv::move_files(
        &[src.join("old").join("target.ts").to_string_lossy().to_string()],
        src.join("new").to_string_lossy().as_ref(),
        &options,
    )
    .unwrap();

    let content = fs::read_to_string(consumer.join("init.ts")).unwrap();
    assert!(
        content.contains("import '../new/target'"),
        "Side-effect import should update. Got:\n{content}"
    );
}

#[test]
fn test_re_export() {
    let dir = TempDir::new().unwrap();
    setup_project(&dir);

    let src = dir.path().join("src");
    let consumer = src.join("consumer");

    // Re-export: export { x } from '...'
    fs::write(
        consumer.join("re-exports.ts"),
        "export { x } from '../old/target';\n",
    )
    .unwrap();

    let options = MoveOptions {
        force: true,
        verbose: false,
        absolute_imports: false,
        ..MoveOptions::default()
    };

    tsmv::move_files(
        &[src.join("old").join("target.ts").to_string_lossy().to_string()],
        src.join("new").to_string_lossy().as_ref(),
        &options,
    )
    .unwrap();

    let content = fs::read_to_string(consumer.join("re-exports.ts")).unwrap();
    assert!(
        content.contains("export { x } from '../new/target'"),
        "Re-export should update. Got:\n{content}"
    );
}

#[test]
fn test_export_star() {
    let dir = TempDir::new().unwrap();
    setup_project(&dir);

    let src = dir.path().join("src");
    let consumer = src.join("consumer");

    // Export * from '...'
    fs::write(
        consumer.join("all.ts"),
        "export * from '../old/target';\n",
    )
    .unwrap();

    let options = MoveOptions {
        force: true,
        verbose: false,
        absolute_imports: false,
        ..MoveOptions::default()
    };

    tsmv::move_files(
        &[src.join("old").join("target.ts").to_string_lossy().to_string()],
        src.join("new").to_string_lossy().as_ref(),
        &options,
    )
    .unwrap();

    let content = fs::read_to_string(consumer.join("all.ts")).unwrap();
    assert!(
        content.contains("export * from '../new/target'"),
        "export * should update. Got:\n{content}"
    );
}

#[test]
fn test_default_plus_named_import() {
    let dir = TempDir::new().unwrap();
    setup_project(&dir);

    let src = dir.path().join("src");
    let consumer = src.join("consumer");

    fs::write(
        consumer.join("combo.ts"),
        "import def, { x } from '../old/target';\nexport const combo = def + x;\n",
    )
    .unwrap();

    let options = MoveOptions {
        force: true,
        verbose: false,
        absolute_imports: false,
        ..MoveOptions::default()
    };

    tsmv::move_files(
        &[src.join("old").join("target.ts").to_string_lossy().to_string()],
        src.join("new").to_string_lossy().as_ref(),
        &options,
    )
    .unwrap();

    let content = fs::read_to_string(consumer.join("combo.ts")).unwrap();
    assert!(
        content.contains("import def, { x } from '../new/target'"),
        "Default+named import should update. Got:\n{content}"
    );
}
