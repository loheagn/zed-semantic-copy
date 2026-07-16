pub mod clipboard;
pub mod context;
pub mod formatter;
pub mod go_semantics;
pub mod install;

use anyhow::Result;

use crate::{clipboard::ClipboardSink, context::CopyContext, formatter::format_selection};

/// Formats the current Zed selection and writes it to the supplied clipboard.
///
/// Returning the formatted value keeps the orchestration testable without
/// reading the real system clipboard.
pub fn copy_selection(context: &CopyContext, clipboard: &mut impl ClipboardSink) -> Result<String> {
    let output = format_selection(context);
    clipboard.write(&output)?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[derive(Default)]
    struct FakeClipboard {
        writes: Vec<String>,
    }

    impl ClipboardSink for FakeClipboard {
        fn write(&mut self, text: &str) -> Result<()> {
            self.writes.push(text.to_owned());
            Ok(())
        }
    }

    #[test]
    fn copy_writes_the_exact_formatted_text_once() {
        let context = CopyContext {
            selected_text: "value".to_owned(),
            start_line: 9,
            start_column: 1,
            filename: "main.txt".to_owned(),
            relative_path: "docs/main.txt".to_owned(),
            absolute_path: Some(PathBuf::from("/repo/docs/main.txt")),
            language: Some("Plain Text".to_owned()),
            outline_symbol: None,
        };
        let mut clipboard = FakeClipboard::default();

        let output = copy_selection(&context, &mut clipboard).unwrap();

        assert_eq!(output, "[main.txt](docs/main.txt) 第 9 行的 value");
        assert_eq!(clipboard.writes, vec![output]);
    }
}
