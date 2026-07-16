use std::{
    env, fmt,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, Result, bail};
use serde_json::json;

pub const TASK_LABEL: &str = "zed-semantic-copy: copy selection";
pub const SECONDARY_KEYBINDING: &str = "cmd-shift-c";
const BINARY_NAME: &str = "zed-semantic-copy";
const TASK_BEGIN: &str = "// zed-semantic-copy:begin task v1";
const TASK_END: &str = "// zed-semantic-copy:end task v1";
const KEYMAP_BEGIN: &str = "// zed-semantic-copy:begin keymap v1";
const KEYMAP_END: &str = "// zed-semantic-copy:end keymap v1";

#[derive(Clone, Debug)]
pub struct InstallOptions {
    pub config_dir: PathBuf,
    pub bin_dir: PathBuf,
    pub source_executable: PathBuf,
    pub keybinding: String,
    pub dry_run: bool,
}

impl InstallOptions {
    pub fn for_current_user(
        config_dir: Option<PathBuf>,
        bin_dir: Option<PathBuf>,
        keybinding: String,
        dry_run: bool,
    ) -> Result<Self> {
        let (config_dir, bin_dir) = user_paths(config_dir, bin_dir)?;
        Ok(Self {
            config_dir,
            bin_dir,
            source_executable: env::current_exe()
                .context("failed to locate the current executable")?,
            keybinding,
            dry_run,
        })
    }
}

#[derive(Clone, Debug)]
pub struct UninstallOptions {
    pub config_dir: PathBuf,
    pub bin_dir: PathBuf,
    pub keep_binary: bool,
    pub dry_run: bool,
}

impl UninstallOptions {
    pub fn for_current_user(
        config_dir: Option<PathBuf>,
        bin_dir: Option<PathBuf>,
        keep_binary: bool,
        dry_run: bool,
    ) -> Result<Self> {
        let (config_dir, bin_dir) = user_paths(config_dir, bin_dir)?;
        Ok(Self {
            config_dir,
            bin_dir,
            keep_binary,
            dry_run,
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InstallReport {
    pub changes: Vec<String>,
}

impl InstallReport {
    fn record(&mut self, message: impl Into<String>) {
        self.changes.push(message.into());
    }
}

impl fmt::Display for InstallReport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.changes.is_empty() {
            return formatter.write_str("No changes were needed.");
        }
        formatter.write_str(&self.changes.join("\n"))
    }
}

pub fn install(options: &InstallOptions) -> Result<InstallReport> {
    if options.keybinding.trim().is_empty() {
        bail!("keybinding must not be empty");
    }
    if options.keybinding == SECONDARY_KEYBINDING {
        bail!(
            "the Vim keybinding must differ from the built-in secondary keybinding {SECONDARY_KEYBINDING}"
        );
    }
    if !options.source_executable.is_file() {
        bail!(
            "source executable does not exist: {}",
            options.source_executable.display()
        );
    }

    let destination = options.bin_dir.join(BINARY_NAME);
    let tasks_path = options.config_dir.join("tasks.json");
    let keymap_path = options.config_dir.join("keymap.json");
    let tasks_original = read_config_or_empty(&tasks_path)?;
    let keymap_original = read_config_or_empty(&keymap_path)?;

    let task_without_managed = remove_managed_block(&tasks_original, TASK_BEGIN, TASK_END)?.0;
    if task_without_managed.contains(TASK_LABEL) {
        bail!("tasks.json already contains a non-managed task named {TASK_LABEL:?}");
    }
    let keymap_without_managed =
        remove_managed_block(&keymap_original, KEYMAP_BEGIN, KEYMAP_END)?.0;
    for keybinding in [&options.keybinding, SECONDARY_KEYBINDING] {
        let quoted_key = serde_json::to_string(keybinding)?;
        if contains_json_key(&keymap_without_managed, &quoted_key) {
            bail!("keymap.json already binds {keybinding}");
        }
    }

    let task = json!({
        "label": TASK_LABEL,
        "command": path_as_utf8(&destination)?,
        "args": [],
        "use_new_terminal": false,
        "allow_concurrent_runs": false,
        "reveal": "never",
        "hide": "on_success",
        "shell": "system",
        "show_summary": false,
        "show_command": false,
        "save": "none",
    });
    let vim_keymap = json!({
        "context": "Editor && VimControl && !VimWaiting && !menu",
        "bindings": {
            options.keybinding.clone(): [
                "task::Spawn",
                { "task_name": TASK_LABEL },
            ],
        },
    });
    let editor_keymap = json!({
        "context": "Editor && !menu",
        "bindings": {
            SECONDARY_KEYBINDING: [
                "task::Spawn",
                { "task_name": TASK_LABEL },
            ],
        },
    });
    let keymap_entries = format!(
        "{},\n{}",
        serde_json::to_string_pretty(&vim_keymap)?,
        serde_json::to_string_pretty(&editor_keymap)?,
    );
    let tasks_updated = upsert_managed_block(
        &tasks_original,
        TASK_BEGIN,
        TASK_END,
        &serde_json::to_string_pretty(&task)?,
    )?;
    let keymap_updated =
        upsert_managed_block(&keymap_original, KEYMAP_BEGIN, KEYMAP_END, &keymap_entries)?;

    let mut report = InstallReport::default();
    let binary_changed = !files_equal(&options.source_executable, &destination)?;
    if binary_changed {
        if destination.exists() && !tasks_original.contains(TASK_BEGIN) {
            bail!(
                "refusing to overwrite an unmanaged executable at {}",
                destination.display()
            );
        }
        report.record(action_message(options.dry_run, "Install", &destination));
    }
    if tasks_updated != tasks_original {
        report.record(action_message(options.dry_run, "Update", &tasks_path));
    }
    if keymap_updated != keymap_original {
        report.record(action_message(options.dry_run, "Update", &keymap_path));
    }

    if options.dry_run {
        return Ok(report);
    }

    if binary_changed {
        fs::create_dir_all(&options.bin_dir)
            .with_context(|| format!("failed to create {}", options.bin_dir.display()))?;
        atomic_copy(&options.source_executable, &destination)?;
    }
    if tasks_updated != tasks_original {
        backup_once(&tasks_path)?;
        atomic_write(&tasks_path, tasks_updated.as_bytes())?;
    }
    if keymap_updated != keymap_original {
        backup_once(&keymap_path)?;
        atomic_write(&keymap_path, keymap_updated.as_bytes())?;
    }

    Ok(report)
}

pub fn uninstall(options: &UninstallOptions) -> Result<InstallReport> {
    let tasks_path = options.config_dir.join("tasks.json");
    let keymap_path = options.config_dir.join("keymap.json");
    let destination = options.bin_dir.join(BINARY_NAME);
    let mut report = InstallReport::default();

    let tasks_update = remove_block_from_file(&tasks_path, TASK_BEGIN, TASK_END)?;
    let keymap_update = remove_block_from_file(&keymap_path, KEYMAP_BEGIN, KEYMAP_END)?;
    if tasks_update.is_some() {
        report.record(action_message(options.dry_run, "Update", &tasks_path));
    }
    if keymap_update.is_some() {
        report.record(action_message(options.dry_run, "Update", &keymap_path));
    }
    if !options.keep_binary && destination.exists() {
        report.record(action_message(options.dry_run, "Remove", &destination));
    }

    if options.dry_run {
        return Ok(report);
    }
    if let Some(contents) = tasks_update {
        atomic_write(&tasks_path, contents.as_bytes())?;
    }
    if let Some(contents) = keymap_update {
        atomic_write(&keymap_path, contents.as_bytes())?;
    }
    if !options.keep_binary && destination.exists() {
        fs::remove_file(&destination)
            .with_context(|| format!("failed to remove {}", destination.display()))?;
    }
    Ok(report)
}

fn user_paths(config_dir: Option<PathBuf>, bin_dir: Option<PathBuf>) -> Result<(PathBuf, PathBuf)> {
    if let (Some(config_dir), Some(bin_dir)) = (config_dir.as_ref(), bin_dir.as_ref()) {
        return Ok((config_dir.clone(), bin_dir.clone()));
    }
    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .context("HOME is not set; pass --config-dir and --bin-dir explicitly")?;
    Ok((
        config_dir.unwrap_or_else(|| home.join(".config/zed")),
        bin_dir.unwrap_or_else(|| home.join(".local/bin")),
    ))
}

fn path_as_utf8(path: &Path) -> Result<&str> {
    path.to_str()
        .with_context(|| format!("path is not valid UTF-8: {}", path.display()))
}

fn read_config_or_empty(path: &Path) -> Result<String> {
    if !path.exists() {
        return Ok("[\n]\n".to_owned());
    }
    fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))
}

fn remove_block_from_file(path: &Path, begin: &str, end: &str) -> Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }
    let original =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let (updated, removed) = remove_managed_block(&original, begin, end)?;
    Ok(removed.then_some(updated))
}

fn action_message(dry_run: bool, action: &str, path: &Path) -> String {
    if dry_run {
        format!("Would {action}: {}", path.display())
    } else {
        format!("{action}: {}", path.display())
    }
}

fn contains_json_key(input: &str, quoted_key: &str) -> bool {
    input.match_indices(quoted_key).any(|(index, _)| {
        input[index + quoted_key.len()..]
            .trim_start_matches(char::is_whitespace)
            .starts_with(':')
    })
}

fn files_equal(left: &Path, right: &Path) -> Result<bool> {
    if !right.exists() {
        return Ok(false);
    }
    let left_bytes =
        fs::read(left).with_context(|| format!("failed to read {}", left.display()))?;
    let right_bytes =
        fs::read(right).with_context(|| format!("failed to read {}", right.display()))?;
    Ok(left_bytes == right_bytes)
}

fn backup_once(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("configuration filename is not valid UTF-8")?;
    let backup = path.with_file_name(format!("{filename}.zed-semantic-copy.bak"));
    if !backup.exists() {
        fs::copy(path, &backup).with_context(|| {
            format!(
                "failed to back up {} to {}",
                path.display(),
                backup.display()
            )
        })?;
    }
    Ok(())
}

fn atomic_copy(source: &Path, destination: &Path) -> Result<()> {
    let bytes = fs::read(source).with_context(|| format!("failed to read {}", source.display()))?;
    atomic_write(destination, &bytes)?;
    let permissions = fs::metadata(source)
        .with_context(|| format!("failed to inspect {}", source.display()))?
        .permissions();
    fs::set_permissions(destination, permissions)
        .with_context(|| format!("failed to set permissions on {}", destination.display()))
}

fn atomic_write(path: &Path, contents: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .with_context(|| format!("{} has no parent directory", path.display()))?;
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("target filename is not valid UTF-8")?;
    let temporary = parent.join(format!(".{filename}.{}.tmp", std::process::id()));
    let previous_permissions = fs::metadata(path)
        .ok()
        .map(|metadata| metadata.permissions());

    let result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .with_context(|| format!("failed to create {}", temporary.display()))?;
        file.write_all(contents)
            .with_context(|| format!("failed to write {}", temporary.display()))?;
        file.sync_all()
            .with_context(|| format!("failed to sync {}", temporary.display()))?;
        drop(file);
        if let Some(permissions) = previous_permissions {
            fs::set_permissions(&temporary, permissions).with_context(|| {
                format!("failed to preserve permissions for {}", path.display())
            })?;
        }
        fs::rename(&temporary, path).with_context(|| {
            format!(
                "failed to replace {} with {}",
                path.display(),
                temporary.display()
            )
        })?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn upsert_managed_block(input: &str, begin: &str, end: &str, object: &str) -> Result<String> {
    let bounds = root_array_bounds(input)?;
    let marker = marker_range(input, begin, end)?;
    if let Some((start, finish)) = marker {
        let old_block = &input[start..finish];
        let leading_comma = structural_tokens(old_block)?
            .first()
            .is_some_and(|(_, token)| *token == b',');
        let newline = newline_style(input);
        let new_block = managed_block(begin, end, object, leading_comma, newline);
        let mut output = String::with_capacity(input.len() - old_block.len() + new_block.len());
        output.push_str(&input[..start]);
        output.push_str(&new_block);
        output.push_str(&input[finish..]);
        root_array_bounds(&output)?;
        return Ok(output);
    }

    let tokens = structural_tokens(&input[bounds.0 + 1..bounds.1])?;
    let leading_comma = tokens.last().is_some_and(|(_, token)| *token != b',');
    let newline = newline_style(input);
    let block = managed_block(begin, end, object, leading_comma, newline);
    let before_close = &input[..bounds.1];
    let prefix_newline = if before_close.ends_with(['\n', '\r']) {
        ""
    } else {
        newline
    };
    let mut output = String::with_capacity(input.len() + block.len() + newline.len());
    output.push_str(before_close);
    output.push_str(prefix_newline);
    output.push_str(&block);
    output.push_str(&input[bounds.1..]);
    root_array_bounds(&output)?;
    Ok(output)
}

fn remove_managed_block(input: &str, begin: &str, end: &str) -> Result<(String, bool)> {
    root_array_bounds(input)?;
    let Some((start, finish)) = marker_range(input, begin, end)? else {
        return Ok((input.to_owned(), false));
    };
    let mut output = String::with_capacity(input.len() - (finish - start));
    output.push_str(&input[..start]);
    let seam = output.len();
    output.push_str(&input[finish..]);

    let tokens = structural_tokens(&output)?;
    let previous = tokens.iter().rev().find(|(position, _)| *position < seam);
    let next = tokens.iter().find(|(position, _)| *position >= seam);
    if let (Some((_, previous)), Some((next_position, next))) = (previous, next)
        && *next == b','
        && matches!(*previous, b'[' | b',')
    {
        output.remove(*next_position);
    }
    root_array_bounds(&output)?;
    Ok((output, true))
}

fn managed_block(
    begin: &str,
    end: &str,
    object: &str,
    leading_comma: bool,
    newline: &str,
) -> String {
    let mut indented = object
        .lines()
        .map(|line| format!("  {line}"))
        .collect::<Vec<_>>();
    if leading_comma {
        indented[0].insert(2, ',');
    }
    format!(
        "  {begin}{newline}{}{newline}  {end}{newline}",
        indented.join(newline)
    )
}

fn marker_range(input: &str, begin: &str, end: &str) -> Result<Option<(usize, usize)>> {
    let begin_matches = input.match_indices(begin).collect::<Vec<_>>();
    let end_matches = input.match_indices(end).collect::<Vec<_>>();
    match (begin_matches.as_slice(), end_matches.as_slice()) {
        ([], []) => Ok(None),
        ([(begin_index, _)], [(end_index, _)]) if begin_index < end_index => {
            let start = input[..*begin_index]
                .rfind('\n')
                .map_or(0, |newline| newline + 1);
            let marker_end = end_index + end.len();
            let finish = input[marker_end..]
                .find('\n')
                .map_or(input.len(), |newline| marker_end + newline + 1);
            Ok(Some((start, finish)))
        }
        _ => bail!("managed markers are duplicated, incomplete, or out of order"),
    }
}

fn newline_style(input: &str) -> &'static str {
    if input.contains("\r\n") { "\r\n" } else { "\n" }
}

fn root_array_bounds(input: &str) -> Result<(usize, usize)> {
    let tokens = structural_tokens(input)?;
    let Some((open, first)) = tokens.first() else {
        bail!("Zed configuration is empty; expected a root JSONC array");
    };
    if *first != b'[' {
        bail!("Zed configuration must be a root JSONC array");
    }

    let mut depth = 0_usize;
    for (index, (position, token)) in tokens.iter().enumerate() {
        match token {
            b'[' => depth += 1,
            b']' => {
                depth = depth
                    .checked_sub(1)
                    .context("unexpected closing array bracket")?;
                if depth == 0 {
                    if index + 1 != tokens.len() {
                        bail!("unexpected content after the root JSONC array");
                    }
                    return Ok((*open, *position));
                }
            }
            _ => {}
        }
    }
    bail!("unterminated root JSONC array")
}

#[derive(Clone, Copy)]
enum LexState {
    Normal,
    String { escaped: bool },
    LineComment,
    BlockComment,
}

fn structural_tokens(input: &str) -> Result<Vec<(usize, u8)>> {
    let bytes = input.as_bytes();
    let mut tokens = Vec::new();
    let mut state = LexState::Normal;
    let mut index = 0;
    while index < bytes.len() {
        match state {
            LexState::Normal => match bytes[index] {
                b'"' => state = LexState::String { escaped: false },
                b'/' if bytes.get(index + 1) == Some(&b'/') => {
                    state = LexState::LineComment;
                    index += 1;
                }
                b'/' if bytes.get(index + 1) == Some(&b'*') => {
                    state = LexState::BlockComment;
                    index += 1;
                }
                byte if byte.is_ascii_whitespace() => {}
                byte => tokens.push((index, byte)),
            },
            LexState::String { escaped } => {
                if escaped {
                    state = LexState::String { escaped: false };
                } else if bytes[index] == b'\\' {
                    state = LexState::String { escaped: true };
                } else if bytes[index] == b'"' {
                    state = LexState::Normal;
                }
            }
            LexState::LineComment => {
                if bytes[index] == b'\n' {
                    state = LexState::Normal;
                }
            }
            LexState::BlockComment => {
                if bytes[index] == b'*' && bytes.get(index + 1) == Some(&b'/') {
                    state = LexState::Normal;
                    index += 1;
                }
            }
        }
        index += 1;
    }
    match state {
        LexState::Normal | LexState::LineComment => Ok(tokens),
        LexState::String { .. } => bail!("unterminated JSONC string"),
        LexState::BlockComment => bail!("unterminated JSONC block comment"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_updates_and_removes_a_managed_block() {
        let original = "[\n  // keep this\n  {\"label\": \"existing\"},\n]\n";
        let installed =
            upsert_managed_block(original, TASK_BEGIN, TASK_END, "{\n  \"x\": 1\n}").unwrap();
        assert!(installed.contains("// keep this"));
        assert_eq!(installed.matches(TASK_BEGIN).count(), 1);

        let reinstalled =
            upsert_managed_block(&installed, TASK_BEGIN, TASK_END, "{\n  \"x\": 1\n}").unwrap();
        assert_eq!(reinstalled, installed);

        let (uninstalled, removed) =
            remove_managed_block(&installed, TASK_BEGIN, TASK_END).unwrap();
        assert!(removed);
        assert_eq!(uninstalled, original);
    }

    #[test]
    fn handles_empty_arrays_without_trailing_newlines() {
        let installed =
            upsert_managed_block("[]", TASK_BEGIN, TASK_END, "{\n  \"x\": 1\n}").unwrap();
        root_array_bounds(&installed).unwrap();
        let (uninstalled, _) = remove_managed_block(&installed, TASK_BEGIN, TASK_END).unwrap();
        assert_eq!(uninstalled, "[\n]");
    }

    #[test]
    fn ignores_brackets_in_strings_and_comments() {
        let original = "[\n  // ]\n  {\"value\": \"not ] the end\"}\n]\n";
        let installed = upsert_managed_block(original, TASK_BEGIN, TASK_END, "{\"x\":1}").unwrap();
        root_array_bounds(&installed).unwrap();
        assert!(installed.contains("not ] the end"));
    }

    #[test]
    fn rejects_incomplete_or_duplicate_markers() {
        assert!(
            upsert_managed_block(
                "[\n// zed-semantic-copy:begin task v1\n]\n",
                TASK_BEGIN,
                TASK_END,
                "{}"
            )
            .is_err()
        );
    }

    #[test]
    fn install_is_idempotent_and_uninstall_preserves_other_config() {
        let directory = tempfile::tempdir().unwrap();
        let config_dir = directory.path().join("config");
        let bin_dir = directory.path().join("bin");
        let source = directory.path().join("source-binary");
        fs::write(&source, b"binary").unwrap();
        fs::create_dir_all(&config_dir).unwrap();
        fs::write(
            config_dir.join("tasks.json"),
            "[\n  // user task\n  {\"label\":\"mine\"},\n]\n",
        )
        .unwrap();
        fs::write(config_dir.join("keymap.json"), "[\n]\n").unwrap();
        let options = InstallOptions {
            config_dir: config_dir.clone(),
            bin_dir: bin_dir.clone(),
            source_executable: source,
            keybinding: "space y s".to_owned(),
            dry_run: false,
        };

        install(&options).unwrap();
        let tasks_once = fs::read(config_dir.join("tasks.json")).unwrap();
        let keymap_once = fs::read(config_dir.join("keymap.json")).unwrap();
        let keymap_text = String::from_utf8(keymap_once.clone()).unwrap();
        assert!(keymap_text.contains("\"space y s\""));
        assert!(keymap_text.contains("\"cmd-shift-c\""));
        assert!(keymap_text.contains("\"context\": \"Editor && !menu\""));
        install(&options).unwrap();
        assert_eq!(fs::read(config_dir.join("tasks.json")).unwrap(), tasks_once);
        assert_eq!(
            fs::read(config_dir.join("keymap.json")).unwrap(),
            keymap_once
        );

        fs::write(
            config_dir.join("keymap.json"),
            format!(
                "{}// later user comment\n",
                String::from_utf8(keymap_once).unwrap()
            ),
        )
        .unwrap();
        uninstall(&UninstallOptions {
            config_dir: config_dir.clone(),
            bin_dir: bin_dir.clone(),
            keep_binary: false,
            dry_run: false,
        })
        .unwrap();
        let tasks = fs::read_to_string(config_dir.join("tasks.json")).unwrap();
        let keymap = fs::read_to_string(config_dir.join("keymap.json")).unwrap();
        assert!(tasks.contains("user task"));
        assert!(!tasks.contains(TASK_BEGIN));
        assert!(keymap.contains("later user comment"));
        assert!(!keymap.contains(KEYMAP_BEGIN));
        assert!(!bin_dir.join(BINARY_NAME).exists());
    }

    #[test]
    fn dry_run_does_not_touch_the_filesystem() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source-binary");
        fs::write(&source, b"binary").unwrap();
        let options = InstallOptions {
            config_dir: directory.path().join("missing-config"),
            bin_dir: directory.path().join("missing-bin"),
            source_executable: source,
            keybinding: "space y s".to_owned(),
            dry_run: true,
        };
        let report = install(&options).unwrap();
        assert!(!report.changes.is_empty());
        assert!(!options.config_dir.exists());
        assert!(!options.bin_dir.exists());
    }

    #[test]
    fn rejects_an_existing_secondary_keybinding() {
        let directory = tempfile::tempdir().unwrap();
        let config_dir = directory.path().join("config");
        let source = directory.path().join("source-binary");
        fs::create_dir_all(&config_dir).unwrap();
        fs::write(&source, b"binary").unwrap();
        fs::write(config_dir.join("tasks.json"), "[]\n").unwrap();
        fs::write(
            config_dir.join("keymap.json"),
            r#"[{"bindings":{"cmd-shift-c":"editor::Copy"}}]"#,
        )
        .unwrap();
        let options = InstallOptions {
            config_dir,
            bin_dir: directory.path().join("bin"),
            source_executable: source,
            keybinding: "space y s".to_owned(),
            dry_run: true,
        };

        let error = install(&options).unwrap_err();
        assert!(error.to_string().contains("cmd-shift-c"));
    }
}
