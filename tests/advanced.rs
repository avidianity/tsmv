use std::fs;

use tempfile::TempDir;

use tsmv::options::MoveOptions;

#[test]
fn test_deeply_nested_directory_move() {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");

    // Create deep nesting: src/a/b/c/d/
    let deep = src.join("a").join("b").join("c").join("d");
    fs::create_dir_all(&deep).unwrap();

    // File at depth 4
    fs::write(deep.join("deep-file.ts"), "export const x = 42;\n").unwrap();

    // Importer at src/ level (goes up 4 levels, then down)
    let consumer_dir = src.join("consumer");
    fs::create_dir_all(&consumer_dir).unwrap();
    fs::write(
        consumer_dir.join("app.ts"),
        "import { x } from '../a/b/c/d/deep-file';\nexport const y = x;\n",
    )
    .unwrap();

    fs::write(
        dir.path().join("tsconfig.json"),
        r#"{"compilerOptions":{"target":"es2020"},"include":["src/**/*"]}"#,
    )
    .unwrap();

    let flat_dir = src.join("flat");

    let options = MoveOptions {
        force: true,
        verbose: false,
        absolute_imports: false,
        ..MoveOptions::default()
    };

    let sources = vec![deep.join("deep-file.ts").to_string_lossy().to_string()];
    tsmv::move_files(&sources, flat_dir.to_string_lossy().as_ref(), &options).unwrap();

    assert!(flat_dir.join("deep-file.ts").exists());

    let app = fs::read_to_string(consumer_dir.join("app.ts")).unwrap();
    assert!(
        app.contains("from '../flat/deep-file'"),
        "Deep import should update. Got:\n{app}"
    );
}

#[test]
fn test_file_rename() {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();

    fs::write(src.join("old-name.ts"), "export const old = true;\n").unwrap();

    // Another file that imports old-name
    fs::write(
        src.join("app.ts"),
        "import { old } from './old-name';\nexport const app = old;\n",
    )
    .unwrap();

    fs::write(
        dir.path().join("tsconfig.json"),
        r#"{"compilerOptions":{"target":"es2020"},"include":["src/**/*"]}"#,
    )
    .unwrap();

    let options = MoveOptions {
        force: true,
        verbose: false,
        absolute_imports: false,
        ..MoveOptions::default()
    };

    // Rename: destination has .ts extension → file rename
    let new_name = src.join("new-name.ts");
    let sources = vec![src.join("old-name.ts").to_string_lossy().to_string()];
    tsmv::move_files(&sources, new_name.to_string_lossy().as_ref(), &options).unwrap();

    assert!(!src.join("old-name.ts").exists());
    assert!(new_name.exists());

    let app = fs::read_to_string(src.join("app.ts")).unwrap();
    assert!(
        app.contains("from './new-name'"),
        "Import should reference new name. Got:\n{app}"
    );
}

#[test]
fn test_force_overwrite_existing_file() {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    let dest_dir = dir.path().join("dest");
    fs::create_dir_all(&src).unwrap();
    fs::create_dir_all(&dest_dir).unwrap();

    fs::write(src.join("file.ts"), "export const new_content = 1;\n").unwrap();
    fs::write(dest_dir.join("file.ts"), "export const old_content = 0;\n").unwrap();

    fs::write(
        dir.path().join("tsconfig.json"),
        r#"{"compilerOptions":{"target":"es2020"},"include":["src/**/*"]}"#,
    )
    .unwrap();

    // Without force: should fail
    let options_no_force = MoveOptions {
        force: false,
        verbose: false,
        ..MoveOptions::default()
    };
    let sources = vec![src.join("file.ts").to_string_lossy().to_string()];
    let result = tsmv::move_files(&sources, dest_dir.to_string_lossy().as_ref(), &options_no_force).unwrap();
    assert!(!result.errors.is_empty(), "should have error about destination existing");
    assert!(src.join("file.ts").exists(), "source should still exist");

    // With force: should overwrite
    let options_force = MoveOptions {
        force: true,
        verbose: false,
        ..MoveOptions::default()
    };
    tsmv::move_files(&sources, dest_dir.to_string_lossy().as_ref(), &options_force).unwrap();

    assert!(!src.join("file.ts").exists(), "source should be moved");
    let dest_content = fs::read_to_string(dest_dir.join("file.ts")).unwrap();
    assert_eq!(dest_content.trim(), "export const new_content = 1;");
}

#[test]
fn test_cross_move_both_files_move() {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    let old_a = src.join("old-a");
    let old_b = src.join("old-b");
    let new_a = src.join("new-a");
    let new_b = src.join("new-b");
    fs::create_dir_all(&old_a).unwrap();
    fs::create_dir_all(&old_b).unwrap();
    fs::create_dir_all(&new_a).unwrap();
    fs::create_dir_all(&new_b).unwrap();

    // old-a/helper.ts is imported by old-b/consumer.ts
    fs::write(old_a.join("helper.ts"), "export const helper = 42;\n").unwrap();
    fs::write(
        old_b.join("consumer.ts"),
        "import { helper } from '../old-a/helper';\nexport const double = helper * 2;\n",
    )
    .unwrap();

    fs::write(
        dir.path().join("tsconfig.json"),
        r#"{"compilerOptions":{"target":"es2020"},"include":["src/**/*"]}"#,
    )
    .unwrap();

    // Move both to new locations
    let options = MoveOptions {
        force: true,
        verbose: false,
        absolute_imports: false,
        ..MoveOptions::default()
    };

    tsmv::move_files(
        &[old_a.join("helper.ts").to_string_lossy().to_string()],
        new_a.to_string_lossy().as_ref(),
        &options,
    )
    .unwrap();

    tsmv::move_files(
        &[old_b.join("consumer.ts").to_string_lossy().to_string()],
        new_b.to_string_lossy().as_ref(),
        &options,
    )
    .unwrap();

    // Both files moved
    assert!(new_a.join("helper.ts").exists());
    assert!(new_b.join("consumer.ts").exists());

    // consumer.ts should now import from new-a (relative from new-b)
    let consumer = fs::read_to_string(new_b.join("consumer.ts")).unwrap();
    assert!(
        consumer.contains("from '../new-a/helper'"),
        "Cross-move import should update. Got:\n{consumer}"
    );
}

#[test]
fn test_move_directory_containing_tsconfig() {
    let dir = TempDir::new().unwrap();

    // Create a workspace-like structure with tsconfig at root
    fs::write(
        dir.path().join("tsconfig.json"),
        r#"{"compilerOptions":{"target":"es2020"},"include":["src/**/*"]}"#,
    )
    .unwrap();

    let src = dir.path().join("src");
    let module_a = src.join("module-a");
    let module_b = src.join("module-b");
    fs::create_dir_all(&module_a).unwrap();
    fs::create_dir_all(&module_b).unwrap();

    fs::write(module_a.join("data.ts"), "export const data = [1,2,3];\n").unwrap();
    fs::write(
        module_b.join("use-data.ts"),
        "import { data } from '../module-a/data';\nexport const len = data.length;\n",
    )
    .unwrap();

    let moved_into = src.join("moved");

    let options = MoveOptions {
        force: true,
        verbose: false,
        absolute_imports: false,
        ..MoveOptions::default()
    };

    let sources = vec![module_a.join("data.ts").to_string_lossy().to_string()];
    tsmv::move_files(&sources, moved_into.to_string_lossy().as_ref(), &options).unwrap();

    assert!(moved_into.join("data.ts").exists());
    assert!(!module_a.join("data.ts").exists());

    let use_data = fs::read_to_string(module_b.join("use-data.ts")).unwrap();
    assert!(
        use_data.contains("from '../moved/data'"),
        "Import should point to new location. Got:\n{use_data}"
    );
}
