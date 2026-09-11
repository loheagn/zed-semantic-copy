use std::{
    fs,
    io::Write as _,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_zed-semantic-copy"))
}

fn selected_position(source: &str, selected_text: &str, occurrence: usize) -> (usize, usize) {
    let byte = source
        .match_indices(selected_text)
        .nth(occurrence)
        .map(|(index, _)| index)
        .unwrap_or_else(|| panic!("missing occurrence {occurrence} of {selected_text:?}"));
    let prefix = &source[..byte];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let line_start = prefix.rfind('\n').map_or(0, |index| index + 1);
    (line, byte - line_start + 1)
}

fn stdout_text(output: Output) -> String {
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn copy_zed_disk_source(
    source: &str,
    selected_text: &str,
    occurrence: usize,
    filename: &str,
    relative_path: &str,
    language: &str,
) -> Output {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(filename);
    fs::write(&path, source).unwrap();
    let (start_line, start_column) = selected_position(source, selected_text, occurrence);

    Command::new(binary())
        .args(["copy", "--stdout"])
        .env("ZED_SELECTED_TEXT", selected_text)
        .env("ZED_ROW", start_line.to_string())
        .env("ZED_COLUMN", start_column.to_string())
        .env("ZED_FILE", path)
        .env("ZED_FILENAME", filename)
        .env("ZED_RELATIVE_FILE", relative_path)
        .env("ZED_LANGUAGE", language)
        .output()
        .unwrap()
}

fn copy_editor_json_source(
    source: &str,
    selected_text: &str,
    occurrence: usize,
    filename: &str,
    relative_path: &str,
    language: Option<&str>,
    absolute_path: Option<&Path>,
) -> Output {
    let (start_line, start_column) = selected_position(source, selected_text, occurrence);
    let mut payload = serde_json::json!({
        "selected_text": selected_text,
        "start_line": start_line,
        "start_column": start_column,
        "filename": filename,
        "relative_path": relative_path,
        "document_text": source,
    });
    if let Some(language) = language {
        payload["language"] = serde_json::json!(language);
    }
    if let Some(absolute_path) = absolute_path {
        payload["absolute_path"] = serde_json::json!(absolute_path);
    }

    let mut child = Command::new(binary())
        .args(["copy", "--stdout", "--stdin-json"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(payload.to_string().as_bytes())
        .unwrap();

    child.wait_with_output().unwrap()
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
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
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
fn stdout_mode_classifies_python_symbols_from_zed_disk_source() {
    let source = r#"def helper(value):
    return value

class Greeter:
    def greet(self, name):
        def normalize(value):
            return value.title()
        return normalize(name)
"#;

    assert_eq!(
        stdout_text(copy_zed_disk_source(
            source,
            "Greeter",
            0,
            "greeter.py",
            "pkg/greeter.py",
            "PYTHON"
        )),
        "[greeter.py](pkg/greeter.py) 中的 class Greeter\n"
    );
    assert_eq!(
        stdout_text(copy_zed_disk_source(
            source,
            "greet",
            0,
            "greeter.py",
            "pkg/greeter.py",
            "PYTHON"
        )),
        "[greeter.py](pkg/greeter.py) 中的 method greet\n"
    );
    assert_eq!(
        stdout_text(copy_zed_disk_source(
            source,
            "normalize",
            0,
            "greeter.py",
            "pkg/greeter.py",
            "PYTHON"
        )),
        "[greeter.py](pkg/greeter.py) 中的 function normalize\n"
    );
}

#[test]
fn stdin_json_classifies_unsaved_decorated_async_python_function() {
    let directory = tempfile::tempdir().unwrap();
    let stale_path = directory.path().join("handlers.pyi");
    fs::write(&stale_path, "fetch_data = None\n").unwrap();
    let source = r#"def route(fn):
    return fn

@route
async def fetch_data(client):
    return await client.get("/data")
"#;

    assert_eq!(
        stdout_text(copy_editor_json_source(
            source,
            "fetch_data",
            0,
            "handlers.pyi",
            "app/handlers.pyi",
            None,
            Some(&stale_path)
        )),
        "[handlers.pyi](app/handlers.pyi) 中的 function fetch_data\n"
    );
}

#[test]
fn python_non_declaration_identifiers_fall_back_to_line_references() {
    let source = r#"class Client:
    def request(self, url):
        result = self.session.get(url)
        return result
"#;

    assert_eq!(
        stdout_text(copy_editor_json_source(
            source,
            "url",
            0,
            "client.py",
            "client.py",
            Some("python"),
            None
        )),
        "[client.py](client.py) 第 2 行的 url\n"
    );
    assert_eq!(
        stdout_text(copy_editor_json_source(
            source,
            "result",
            0,
            "client.py",
            "client.py",
            Some("python"),
            None
        )),
        "[client.py](client.py) 第 3 行的 result\n"
    );
    assert_eq!(
        stdout_text(copy_editor_json_source(
            source,
            "get",
            0,
            "client.py",
            "client.py",
            Some("python"),
            None
        )),
        "[client.py](client.py) 第 3 行的 get\n"
    );
}

#[test]
fn python_multiline_selection_uses_line_range() {
    let source = r#"class Greeter:
    def greet(self):
        return "hi"
"#;
    let selected_text = "def greet(self):\n        return \"hi\"";

    assert_eq!(
        stdout_text(copy_editor_json_source(
            source,
            selected_text,
            0,
            "greeter.py",
            "greeter.py",
            Some("python"),
            None
        )),
        "[greeter.py](greeter.py) 第 2 到第 3 行的内容\n"
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
