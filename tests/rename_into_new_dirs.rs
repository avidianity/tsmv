//! Renaming a file into a path whose parent directories do not exist yet.
//!
//! Regression coverage for a bug where a single-file rename planned no
//! directory creation, so the move failed with ENOENT ("No such file or
//! directory") even though the dry-run printed a valid plan.

use std::fs;
use std::path::Path;

use tempfile::TempDir;

/// Build the reported layout: a page that imports a sibling component, plus a
/// consumer that imports the page.
fn setup_web_app(dir: &Path) {
    let src = dir.join("src");
    fs::create_dir_all(src.join("pages")).unwrap();
    fs::create_dir_all(src.join("components")).unwrap();

    fs::write(
        src.join("components").join("panel.tsx"),
        "export function Panel() { return null; }\n",
    )
    .unwrap();

    fs::write(
        src.join("pages").join("ai-assistant.tsx"),
        "import { Panel } from '../components/panel';\nexport default function AiAssistant() { return Panel(); }\n",
    )
    .unwrap();

    fs::write(
        src.join("app.tsx"),
        "import AiAssistant from './pages/ai-assistant';\nexport const App = () => AiAssistant();\n",
    )
    .unwrap();

    fs::write(
        dir.join("tsconfig.json"),
        r#"{"compilerOptions":{"target":"es2020","module":"esnext"},"include":["src/**/*"]}"#,
    )
    .unwrap();
}

#[test]
fn rename_into_missing_nested_directories_succeeds() {
    let dir = TempDir::new().unwrap();
    setup_web_app(dir.path());

    let src_file = dir.path().join("src/pages/ai-assistant.tsx");
    let dest_file = dir
        .path()
        .join("src/features/ai-assistant/pages/assistant.tsx");

    assert!(
        !dir.path().join("src/features").exists(),
        "precondition: the destination tree must not exist yet"
    );

    let mut cmd = assert_cmd::Command::cargo_bin("tsmv").unwrap();
    cmd.args([
        "--no-absolute-imports",
        &src_file.to_string_lossy(),
        &dest_file.to_string_lossy(),
    ])
    .current_dir(dir.path());

    cmd.assert().success();

    assert!(dest_file.exists(), "renamed file should exist at the new path");
    assert!(!src_file.exists(), "original file should be gone");
}

#[test]
fn rename_into_missing_nested_directories_rewrites_imports() {
    let dir = TempDir::new().unwrap();
    setup_web_app(dir.path());

    let src_file = dir.path().join("src/pages/ai-assistant.tsx");
    let dest_file = dir
        .path()
        .join("src/features/ai-assistant/pages/assistant.tsx");

    let mut cmd = assert_cmd::Command::cargo_bin("tsmv").unwrap();
    cmd.args([
        "--no-absolute-imports",
        &src_file.to_string_lossy(),
        &dest_file.to_string_lossy(),
    ])
    .current_dir(dir.path());
    cmd.assert().success();

    // The consumer now points at the new location.
    let app = fs::read_to_string(dir.path().join("src/app.tsx")).unwrap();
    assert!(
        app.contains("./features/ai-assistant/pages/assistant"),
        "consumer import should follow the move. Got:\n{app}"
    );

    // The moved file's own import is recomputed for its new depth.
    let moved = fs::read_to_string(&dest_file).unwrap();
    assert!(
        moved.contains("../../../components/panel"),
        "moved file's own import should be recomputed. Got:\n{moved}"
    );
}

#[test]
fn dry_run_plan_matches_the_real_move() {
    let dir = TempDir::new().unwrap();
    setup_web_app(dir.path());

    let src_file = dir.path().join("src/pages/ai-assistant.tsx");
    let dest_file = dir
        .path()
        .join("src/features/ai-assistant/pages/assistant.tsx");

    let mut dry = assert_cmd::Command::cargo_bin("tsmv").unwrap();
    dry.args([
        "-n",
        &src_file.to_string_lossy(),
        &dest_file.to_string_lossy(),
    ])
    .current_dir(dir.path());
    let output = dry.assert().success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);

    assert!(
        stdout.contains(&*dest_file.to_string_lossy()),
        "dry-run should print the destination. Got:\n{stdout}"
    );
    assert!(!dest_file.exists(), "dry-run must not touch the filesystem");

    // The move the dry-run promised must actually be performable.
    let mut real = assert_cmd::Command::cargo_bin("tsmv").unwrap();
    real.args([
        "--no-absolute-imports",
        &src_file.to_string_lossy(),
        &dest_file.to_string_lossy(),
    ])
    .current_dir(dir.path());
    real.assert().success();

    assert!(dest_file.exists());
}

#[test]
fn deeply_nested_rename_creates_every_level() {
    let dir = TempDir::new().unwrap();
    setup_web_app(dir.path());

    let src_file = dir.path().join("src/pages/ai-assistant.tsx");
    let dest_file = dir.path().join("src/a/b/c/d/e/f/deep.tsx");

    let mut cmd = assert_cmd::Command::cargo_bin("tsmv").unwrap();
    cmd.args([
        "--no-absolute-imports",
        &src_file.to_string_lossy(),
        &dest_file.to_string_lossy(),
    ])
    .current_dir(dir.path());
    cmd.assert().success();

    assert!(dest_file.exists(), "every intermediate level should be created");
}

#[test]
fn rename_with_relative_paths_succeeds() {
    let dir = TempDir::new().unwrap();
    setup_web_app(dir.path());

    // Exercise the path the user hit: relative arguments from the project root.
    let mut cmd = assert_cmd::Command::cargo_bin("tsmv").unwrap();
    cmd.args([
        "--tsconfig",
        "tsconfig.json",
        "--no-absolute-imports",
        "src/pages/ai-assistant.tsx",
        "src/features/ai-assistant/pages/assistant.tsx",
    ])
    .current_dir(dir.path());
    cmd.assert().success();

    assert!(dir
        .path()
        .join("src/features/ai-assistant/pages/assistant.tsx")
        .exists());
}

#[test]
fn failed_move_exits_non_zero() {
    let dir = TempDir::new().unwrap();
    setup_web_app(dir.path());

    // Renaming onto an existing file without --force is refused, and that
    // refusal must be visible to the shell rather than reported as success.
    let src_file = dir.path().join("src/pages/ai-assistant.tsx");
    let dest_file = dir.path().join("src/components/panel.tsx");

    let mut cmd = assert_cmd::Command::cargo_bin("tsmv").unwrap();
    cmd.args([
        &*src_file.to_string_lossy(),
        &*dest_file.to_string_lossy(),
    ])
    .current_dir(dir.path());

    cmd.assert().failure();
    assert!(src_file.exists(), "source must be left alone on failure");
}
