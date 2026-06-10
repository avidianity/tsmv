use tempfile::TempDir;

use tsmv::options::MoveOptions;

/// Helper: create a minimal TypeScript project in a temp directory.
fn setup_temp_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    let utils = src.join("utils");
    let components = src.join("components");

    std::fs::create_dir_all(&utils).unwrap();
    std::fs::create_dir_all(&components).unwrap();

    // Create utils/helpers.ts
    std::fs::write(
        utils.join("helpers.ts"),
        r#"export function toTitleCase(str: string): string {
  return str.replace(/\w\S*/g, (txt) => txt.charAt(0).toUpperCase() + txt.substring(1).toLowerCase());
}
export function generateId(): string {
  return Math.random().toString(36).substring(2, 15);
}
"#,
    )
    .unwrap();

    // Create component that imports from utils
    std::fs::write(
        components.join("Button.ts"),
        r#"import { toTitleCase, generateId } from '../utils/helpers';

interface ButtonProps {
  label: string;
  onClick: () => void;
}

export function Button({ label, onClick }: ButtonProps) {
  const buttonId = generateId();
  return {
    id: buttonId,
    label: toTitleCase(label),
    handleClick: onClick,
  };
}
"#,
    )
    .unwrap();

    // Create tsconfig
    std::fs::write(
        dir.path().join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "es2020",
    "module": "esnext",
    "moduleResolution": "node",
    "esModuleInterop": true,
    "strict": true,
    "outDir": "dist"
  },
  "include": ["src/**/*"]
}"#,
    )
    .unwrap();

    dir
}

#[test]
fn test_move_file_and_update_imports() {
    let dir = setup_temp_project();
    let src = dir.path().join("src");
    let utils = src.join("utils");
    let components = src.join("components");
    let shared = src.join("shared");

    let helpers = utils.join("helpers.ts");

    let options = MoveOptions {
        force: true,
        verbose: false,
        absolute_imports: false,
        ..MoveOptions::default()
    };

    // Move helpers.ts from utils/ to shared/
    let sources = vec![helpers.to_string_lossy().to_string()];
    let dest = shared.to_string_lossy().to_string();

    let result = tsmv::move_files(&sources, &dest, &options).unwrap();

    // File was moved
    assert!(shared.join("helpers.ts").exists(), "helpers.ts should be in shared/");
    assert!(!helpers.exists(), "helpers.ts should no longer exist in utils/");
    assert_eq!(result.moved_files.len(), 1);

    // Import in Button.ts was updated
    let button_content = std::fs::read_to_string(components.join("Button.ts")).unwrap();
    assert!(
        button_content.contains("from '../shared/helpers'"),
        "Import should be updated to '../shared/helpers'\nActual content:\n{button_content}"
    );
    assert!(
        !button_content.contains("from '../utils/helpers'"),
        "Old import should be removed\nActual content:\n{button_content}"
    );
}

#[test]
fn test_dry_run_does_not_modify_files() {
    let dir = setup_temp_project();
    let src = dir.path().join("src");
    let utils = src.join("utils");
    let shared = src.join("shared");

    let helpers = utils.join("helpers.ts");

    let options = MoveOptions {
        dry_run: true,
        verbose: false,
        ..MoveOptions::default()
    };

    let sources = vec![helpers.to_string_lossy().to_string()];
    let dest = shared.to_string_lossy().to_string();

    let result = tsmv::move_files(&sources, &dest, &options).unwrap();

    // Dry run: no files moved
    assert!(helpers.exists(), "Source should still exist after dry-run");
    assert!(!shared.join("helpers.ts").exists(), "Destination should not exist after dry-run");
    assert!(result.moved_files.is_empty());
}

#[test]
fn test_move_multiple_files() {
    let dir = setup_temp_project();
    let src = dir.path().join("src");
    let utils = src.join("utils");
    let shared = src.join("shared");

    // Create a second file in utils
    std::fs::write(
        utils.join("format.ts"),
        r#"export function formatCurrency(amount: number, currency = 'USD'): string {
  return new Intl.NumberFormat('en-US', { style: 'currency', currency }).format(amount);
}
"#,
    )
    .unwrap();

    let helpers = utils.join("helpers.ts");
    let format = utils.join("format.ts");

    let options = MoveOptions {
        force: true,
        verbose: false,
        ..MoveOptions::default()
    };

    let sources = vec![
        helpers.to_string_lossy().to_string(),
        format.to_string_lossy().to_string(),
    ];
    let dest = shared.to_string_lossy().to_string();

    let result = tsmv::move_files(&sources, &dest, &options).unwrap();

    assert!(shared.join("helpers.ts").exists());
    assert!(shared.join("format.ts").exists());
    assert!(!helpers.exists());
    assert!(!format.exists());
    assert_eq!(result.moved_files.len(), 2);
}

#[test]
fn test_move_nonexistent_file_errors() {
    let _dir = TempDir::new().unwrap();

    let options = MoveOptions::default();
    let sources = vec!["/nonexistent/path.ts".to_string()];

    let result = tsmv::move_files(&sources, "/dest", &options);
    assert!(result.is_err());
}
