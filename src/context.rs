use std::{
    env,
    ffi::OsString,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, Result, bail};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CopyContext {
    pub selected_text: String,
    pub start_line: usize,
    pub start_column: usize,
    pub filename: String,
    pub relative_path: String,
    pub absolute_path: Option<PathBuf>,
    pub language: Option<String>,
    pub outline_symbol: Option<String>,
}

impl CopyContext {
    pub fn from_environment() -> Result<Self> {
        Self::from_lookup(|key| env::var_os(key))
    }

    pub fn from_lookup(mut lookup: impl FnMut(&str) -> Option<OsString>) -> Result<Self> {
        let selected_text = required_utf8(&mut lookup, "ZED_SELECTED_TEXT")?;
        if selected_text.trim().is_empty() {
            bail!("the Zed selection is empty or contains only whitespace");
        }

        let start_line = parse_one_based(&required_utf8(&mut lookup, "ZED_ROW")?, "ZED_ROW")?;
        let start_column = optional_utf8(&mut lookup, "ZED_COLUMN")
            .map(|value| parse_one_based(&value, "ZED_COLUMN"))
            .transpose()?
            .unwrap_or(1);

        let absolute_path = optional_utf8(&mut lookup, "ZED_FILE").map(PathBuf::from);
        let relative_path = optional_utf8(&mut lookup, "ZED_RELATIVE_FILE")
            .or_else(|| {
                absolute_path
                    .as_deref()
                    .and_then(Path::file_name)
                    .and_then(|name| name.to_str())
                    .map(ToOwned::to_owned)
            })
            .context("ZED_RELATIVE_FILE and a usable ZED_FILE are both missing")?;
        let filename = optional_utf8(&mut lookup, "ZED_FILENAME")
            .or_else(|| {
                Path::new(&relative_path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(ToOwned::to_owned)
            })
            .context("ZED_FILENAME and a usable relative filename are both missing")?;

        Ok(Self {
            selected_text,
            start_line,
            start_column,
            filename,
            relative_path,
            absolute_path,
            language: optional_utf8(&mut lookup, "ZED_LANGUAGE"),
            outline_symbol: optional_utf8(&mut lookup, "ZED_SYMBOL"),
        })
    }
}

fn required_utf8(lookup: &mut impl FnMut(&str) -> Option<OsString>, key: &str) -> Result<String> {
    optional_utf8(lookup, key)
        .with_context(|| format!("required Zed task variable {key} is missing"))
}

fn optional_utf8(lookup: &mut impl FnMut(&str) -> Option<OsString>, key: &str) -> Option<String> {
    lookup(key).and_then(|value| value.into_string().ok())
}

fn parse_one_based(value: &str, key: &str) -> Result<usize> {
    let parsed = value
        .parse::<usize>()
        .with_context(|| format!("{key} must be a positive integer, got {value:?}"))?;
    if parsed == 0 {
        bail!("{key} must be one-based, got 0");
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn context_from(values: &[(&str, &str)]) -> Result<CopyContext> {
        let values = values
            .iter()
            .map(|(key, value)| ((*key).to_owned(), OsString::from(value)))
            .collect::<HashMap<_, _>>();
        CopyContext::from_lookup(|key| values.get(key).cloned())
    }

    #[test]
    fn reads_zed_task_variables() {
        let context = context_from(&[
            ("ZED_SELECTED_TEXT", "Run"),
            ("ZED_ROW", "12"),
            ("ZED_COLUMN", "7"),
            ("ZED_FILE", "/repo/cmd/main.go"),
            ("ZED_FILENAME", "main.go"),
            ("ZED_RELATIVE_FILE", "cmd/main.go"),
            ("ZED_LANGUAGE", "Go"),
            ("ZED_SYMBOL", "Run"),
        ])
        .unwrap();

        assert_eq!(context.selected_text, "Run");
        assert_eq!(context.start_line, 12);
        assert_eq!(context.start_column, 7);
        assert_eq!(context.relative_path, "cmd/main.go");
        assert_eq!(context.outline_symbol.as_deref(), Some("Run"));
    }

    #[test]
    fn falls_back_to_filename_from_absolute_path() {
        let context = context_from(&[
            ("ZED_SELECTED_TEXT", "value"),
            ("ZED_ROW", "1"),
            ("ZED_FILE", "/tmp/example.go"),
        ])
        .unwrap();

        assert_eq!(context.filename, "example.go");
        assert_eq!(context.relative_path, "example.go");
        assert_eq!(context.start_column, 1);
    }

    #[test]
    fn rejects_empty_selection_and_zero_row() {
        assert!(context_from(&[("ZED_SELECTED_TEXT", "  "), ("ZED_ROW", "1")]).is_err());
        assert!(
            context_from(&[
                ("ZED_SELECTED_TEXT", "x"),
                ("ZED_ROW", "0"),
                ("ZED_RELATIVE_FILE", "x.go"),
            ])
            .is_err()
        );
    }
}
