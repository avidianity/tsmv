use std::fs;
use std::path::Path;

use tempfile::TempDir;

fn setup_ts_project(dir: &Path) {
    let src = dir.join("src");
    let utils = src.join("utils");
    let components = src.join("components");

    fs::create_dir_all(&utils).unwrap();
    fs::create_dir_all(&components).unwrap();

    fs::write(
        utils.join("helpers.ts"),
        "export function helper() { return 1; }\n",
    )
    .unwrap();

    fs::write(
        components.join("Button.ts"),
        "import { helper } from '../utils/helpers';\nexport const Button = () => helper();\n",
    )
    .unwrap();

    fs::write(
        dir.join("tsconfig.json"),
        r#"{"compilerOptions":{"target":"es2020","module":"esnext"},"include":["src/**/*"]}"#,
    )
    .unwrap();
}

#[test]
fn test_cli_move_basic() {
    let dir = TempDir::new().unwrap();
    setup_ts_project(dir.path());

    let src_file = dir.path().join("src").join("utils").join("helpers.ts");
    let dest_dir = dir.path().join("src").join("shared");

    let mut cmd = assert_cmd::Command::cargo_bin("tsmv").unwrap();
    cmd.args([
        "-f",
        "--no-absolute-imports",
        &src_file.to_string_lossy(),
        &dest_dir.to_string_lossy(),
    ])
    .current_dir(dir.path());

    cmd.assert().success();

    assert!(dest_dir.join("helpers.ts").exists(), "file should be moved");
    assert!(!src_file.exists(), "old file should not exist");

    // Import was updated
    let button = fs::read_to_string(dir.path().join("src").join("components").join("Button.ts")).unwrap();
    assert!(
        button.contains("from '../shared/helpers'"),
        "Import should update. Got:\n{button}"
    );
}

#[test]
fn test_cli_dry_run() {
    let dir = TempDir::new().unwrap();
    setup_ts_project(dir.path());

    let src_file = dir.path().join("src").join("utils").join("helpers.ts");
    let dest_dir = dir.path().join("src").join("shared");

    let mut cmd = assert_cmd::Command::cargo_bin("tsmv").unwrap();
    cmd.args([
        "-n",
        &src_file.to_string_lossy(),
        &dest_dir.to_string_lossy(),
    ])
    .current_dir(dir.path());

    let output = cmd.assert().success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);

    assert!(stdout.contains("DRY RUN MODE"));
    assert!(stdout.contains("Files to move:"));

    // No files should be moved
    assert!(src_file.exists(), "source should still exist after dry-run");
    assert!(!dest_dir.join("helpers.ts").exists(), "destination should not exist after dry-run");
}

#[test]
fn test_cli_recursive_directory_move() {
    let dir = TempDir::new().unwrap();
    setup_ts_project(dir.path());

    let utils_dir = dir.path().join("src").join("utils");
    let dest_dir = dir.path().join("src").join("shared");

    let mut cmd = assert_cmd::Command::cargo_bin("tsmv").unwrap();
    cmd.args([
        "-r",
        "-f",
        &utils_dir.to_string_lossy(),
        &dest_dir.to_string_lossy(),
    ])
    .current_dir(dir.path());

    cmd.assert().success();

    // Directory structure preserved
    assert!(dest_dir.join("utils").join("helpers.ts").exists());
    assert!(!utils_dir.join("helpers.ts").exists());
}

#[test]
fn test_cli_missing_args_error() {
    let dir = TempDir::new().unwrap();

    let mut cmd = assert_cmd::Command::cargo_bin("tsmv").unwrap();
    cmd.args(["only_one_arg"])
        .current_dir(dir.path());

    cmd.assert().failure();
}

#[test]
fn test_cli_subcommand_move() {
    let dir = TempDir::new().unwrap();
    setup_ts_project(dir.path());

    let src_file = dir.path().join("src").join("utils").join("helpers.ts");
    let dest_dir = dir.path().join("src").join("shared");

    let mut cmd = assert_cmd::Command::cargo_bin("tsmv").unwrap();
    cmd.args([
        "move",
        "-f",
        "--no-absolute-imports",
        &src_file.to_string_lossy(),
        &dest_dir.to_string_lossy(),
    ])
    .current_dir(dir.path());

    cmd.assert().success();
    assert!(dest_dir.join("helpers.ts").exists());
}
