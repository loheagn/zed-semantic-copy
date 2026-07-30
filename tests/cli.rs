use std::{path::PathBuf, process::Command};

fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_zed-semantic-copy"))
}

#[test]
fn stdout_mode_formats_zed_environment_without_using_clipboard() {
    let output = Command::new(binary())
        .args(["copy", "--stdout"])
        .env("ZED_SELECTED_TEXT", "request")
        .env("ZED_ROW", "42")
        .env("ZED_COLUMN", "3")
        .env("ZED_FILE", "/repo/internal/service.go")
        .env("ZED_FILENAME", "service.go")
        .env("ZED_RELATIVE_FILE", "internal/service.go")
        .env("ZED_LANGUAGE", "Go")
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "[service.go](internal/service.go) 第 42 行的 request\n"
    );
}

#[test]
fn missing_selection_fails_without_touching_clipboard() {
    let output = Command::new(binary())
        .args(["copy", "--stdout"])
        .env_remove("ZED_SELECTED_TEXT")
        .env("ZED_ROW", "1")
        .env("ZED_RELATIVE_FILE", "main.go")
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("ZED_SELECTED_TEXT"));
}

#[test]
fn stdout_mode_accepts_editor_context_json() {
    let mut child = Command::new(binary())
        .args(["copy", "--stdout", "--stdin-json"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write as _;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(
            br#"{
                "selected_text": "Run",
                "start_line": 2,
                "start_column": 6,
                "filename": "main.go",
                "relative_path": "cmd/main.go",
                "language": "go",
                "document_text": "package main\nfunc Run() {}\n"
            }"#,
        )
        .unwrap();

    let output = child.wait_with_output().unwrap();

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "[main.go](cmd/main.go) 中的 function Run\n"
    );
}

#[test]
fn install_dry_run_is_side_effect_free() {
    let directory = tempfile::tempdir().unwrap();
    let config_dir = directory.path().join("config");
    let bin_dir = directory.path().join("bin");
    let output = Command::new(binary())
        .args([
            "install",
            "--dry-run",
            "--config-dir",
            config_dir.to_str().unwrap(),
            "--bin-dir",
            bin_dir.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(!config_dir.exists());
    assert!(!bin_dir.exists());
    assert!(String::from_utf8_lossy(&output.stdout).contains("Would Install"));
}
