use tempfile::TempDir;

use tsmv::options::MoveOptions;

/// Create a temp project with `src/`, `components/` dir with Button.ts, tsconfig.
fn setup_temp_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    let utils = src.join("utils");
    let components = src.join("components");

    std::fs::create_dir_all(&utils).unwrap();
    std::fs::create_dir_all(&components).unwrap();

    std::fs::write(
        utils.join("helpers.ts"),
        "export function helper() { return 1; }\n",
    )
    .unwrap();

    std::fs::write(
        components.join("Button.ts"),
        "import { helper } from '../utils/helpers';\nexport const Button = () => helper();\n",
    )
    .unwrap();

    std::fs::write(
        dir.path().join("tsconfig.json"),
        r#"{"compilerOptions":{"target":"es2020","module":"esnext"},"include":["src/**/*"]}"#,
    )
    .unwrap();

    dir
}

#[test]
fn test_move_directory_preserves_structure() {
    let dir = setup_temp_project();
    let src = dir.path().join("src");
    let utils = src.join("utils");
    let dest = src.join("shared");

    let helpers = utils.join("helpers.ts");

    let options = MoveOptions {
        force: true,
        recursive: true,
        verbose: false,
        ..MoveOptions::default()
    };

    let sources = vec![utils.to_string_lossy().to_string()];
    let result = tsmv::move_files(&sources, dest.to_string_lossy().as_ref(), &options).unwrap();

    // Directory structure preserved: dest/utils/helpers.ts
    assert!(dest.join("utils").join("helpers.ts").exists(),
        "helpers.ts should be in shared/utils/");
    assert!(!helpers.exists(), "old helpers.ts should not exist");
    assert!(!result.errors.is_empty() || !result.moved_files.is_empty());
}

#[test]
fn test_move_directory_without_recursive_fails() {
    let dir = setup_temp_project();
    let src = dir.path().join("src");
    let utils = src.join("utils");
    let dest = src.join("shared");

    let options = MoveOptions {
        force: true,
        recursive: false,
        verbose: false,
        absolute_imports: false,
        ..MoveOptions::default()
    };

    let sources = vec![utils.to_string_lossy().to_string()];
    let result = tsmv::move_files(&sources, dest.to_string_lossy().as_ref(), &options);

    assert!(result.is_err(), "moving a directory without --recursive should fail");
}

#[test]
fn test_self_import_barrel_preservation() {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    let components = src.join("components");
    let cards = src.join("cards");

    std::fs::create_dir_all(&components).unwrap();
    std::fs::create_dir_all(&cards).unwrap();

    // Barrel index.ts that re-exports from siblings
    std::fs::write(
        components.join("index.ts"),
        r#"export { Button } from './Button';
export { Input } from './Input';
"#,
    )
    .unwrap();
    std::fs::write(
        components.join("Button.ts"),
        "export const Button = 'button';\n",
    )
    .unwrap();
    std::fs::write(
        components.join("Input.ts"),
        "export const Input = 'input';\n",
    )
    .unwrap();

    std::fs::write(
        dir.path().join("tsconfig.json"),
        r#"{"compilerOptions":{"target":"es2020"},"include":["src/**/*"]}"#,
    )
    .unwrap();

    let options = MoveOptions {
        force: true,
        recursive: true,
        verbose: false,
        absolute_imports: false,
        ..MoveOptions::default()
    };

    let sources = vec![components.to_string_lossy().to_string()];
    let dest = cards.to_string_lossy().to_string();
    tsmv::move_files(&sources, &dest, &options).unwrap();

    // Barrel file moved
    let barrel = cards.join("components").join("index.ts");
    assert!(barrel.exists(), "barrel index.ts should be moved");

    // Import paths within the moved directory should stay the same (same relative dir)
    let barrel_content = std::fs::read_to_string(&barrel).unwrap();
    assert!(
        barrel_content.contains("from './Button'"),
        "Self-import should stay './Button', got:\n{barrel_content}"
    );
    assert!(
        barrel_content.contains("from './Input'"),
        "Self-import should stay './Input', got:\n{barrel_content}"
    );
}

#[test]
fn test_double_extension_handling() {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    let logic_dir = src.join("logic");
    let new_dir = src.join("services");

    std::fs::create_dir_all(&logic_dir).unwrap();
    std::fs::create_dir_all(&new_dir).unwrap();

    // File with double extension (common for .test.ts, .styles.ts, .logic.ts)
    std::fs::write(
        logic_dir.join("dragDrop.logic.ts"),
        "export const dropHandler = () => {};\n",
    )
    .unwrap();

    std::fs::write(
        logic_dir.join("main.ts"),
        "import { dropHandler } from './dragDrop.logic';\n",
    )
    .unwrap();

    std::fs::write(
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

    let sources = vec![
        logic_dir.join("dragDrop.logic.ts").to_string_lossy().to_string(),
    ];
    let dest = new_dir.to_string_lossy().to_string();

    tsmv::move_files(&sources, &dest, &options).unwrap();

    // File was moved (extension comparison uses Path::extension() = "ts")
    assert!(new_dir.join("dragDrop.logic.ts").exists());

    // Import in main.ts was updated correctly
    let main_content = std::fs::read_to_string(logic_dir.join("main.ts")).unwrap();
    assert!(
        main_content.contains("from '../services/dragDrop.logic'"),
        "Import should be updated to '../services/dragDrop.logic', got:\n{main_content}"
    );
}

#[test]
fn test_index_file_import_resolution() {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    let utils = src.join("utils");
    let new_utils = src.join("helpers");

    std::fs::create_dir_all(&utils).unwrap();
    std::fs::create_dir_all(&new_utils).unwrap();

    // utils/index.ts (barrel file)
    std::fs::write(
        utils.join("index.ts"),
        "export const version = '1.0';\n",
    )
    .unwrap();

    // File importing from the directory (resolves to index.ts)
    std::fs::write(
        src.join("app.ts"),
        "import { version } from './utils';\n",
    )
    .unwrap();

    std::fs::write(
        dir.path().join("tsconfig.json"),
        r#"{"compilerOptions":{"target":"es2020"},"include":["src/**/*"]}"#,
    )
    .unwrap();

    let options = MoveOptions {
        force: true,
        recursive: true,
        verbose: false,
        absolute_imports: false,
        ..MoveOptions::default()
    };

    let sources = vec![utils.to_string_lossy().to_string()];
    let dest = new_utils.to_string_lossy().to_string();

    tsmv::move_files(&sources, &dest, &options).unwrap();

    let app_content = std::fs::read_to_string(src.join("app.ts")).unwrap();
    assert!(
        app_content.contains("from './helpers/utils'"),
        "Import from './utils' (index) should update to './helpers/utils', got:\n{app_content}"
    );
}

#[test]
fn test_custom_extensions_flag() {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    std::fs::create_dir_all(&src).unwrap();

    let js_file = src.join("script.js");

    std::fs::write(
        &js_file,
        "export const foo = 42;\n",
    )
    .unwrap();

    let dest = dir.path().join("dist");

    // Default extensions (.ts, .tsx) should NOT match .js file
    let options = MoveOptions {
        force: true,
        verbose: false,
        ..MoveOptions::default()
    };

    let sources = vec![js_file.to_string_lossy().to_string()];
    let result = tsmv::move_files(&sources, dest.to_string_lossy().as_ref(), &options);

    assert!(result.is_err(), ".js file should not be matched by default extensions");

    // With .js in extensions, it should work
    let options_js = MoveOptions {
        force: true,
        verbose: false,
        extensions: vec![".js".into()],
        ..MoveOptions::default()
    };

    let result_js = tsmv::move_files(&sources, dest.to_string_lossy().as_ref(), &options_js).unwrap();
    assert!(!result_js.moved_files.is_empty());
    assert!(dest.join("script.js").exists());
}

#[test]
fn test_no_files_matched_error() {
    let _dir = TempDir::new().unwrap();

    let options = MoveOptions::default();
    let sources = vec!["nonexistent*.ts".to_string()];

    let result = tsmv::move_files(&sources, "/tmp/dest", &options);
    assert!(result.is_err());
}
