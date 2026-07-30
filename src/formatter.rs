use std::fmt;

use crate::{context::CopyContext, go_semantics};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SymbolKind {
    Function,
    Method,
    Struct,
    Interface,
    Type,
    Constant,
    Field,
    Package,
    Label,
}

impl fmt::Display for SymbolKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Function => "function",
            Self::Method => "method",
            Self::Struct => "struct",
            Self::Interface => "interface",
            Self::Type => "type",
            Self::Constant => "constant",
            Self::Field => "field",
            Self::Package => "package",
            Self::Label => "label",
        };
        formatter.write_str(name)
    }
}

pub fn format_selection(context: &CopyContext) -> String {
    let kind = go_semantics::classify(context);
    format_selection_with_kind(context, kind)
}

pub fn format_selection_with_kind(
    context: &CopyContext,
    symbol_kind: Option<SymbolKind>,
) -> String {
    let file = markdown_file_link(&context.filename, &context.relative_path);
    let (text, line_count) = selected_content_lines(&context.selected_text);

    if line_count > 1 {
        let end_line = context.start_line.saturating_add(line_count - 1);
        return format!("{file} 第 {} 到第 {end_line} 行的内容", context.start_line);
    }

    if let Some(kind) = symbol_kind {
        return format!("{file} 中的 {kind} {}", text.trim());
    }

    format!("{file} 第 {} 行的 {text}", context.start_line)
}

fn selected_content_lines(selection: &str) -> (&str, usize) {
    let content = selection
        .strip_suffix("\r\n")
        .or_else(|| selection.strip_suffix('\n'))
        .or_else(|| selection.strip_suffix('\r'))
        .unwrap_or(selection);
    let bytes = content.as_bytes();
    let mut breaks = 0;
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\r' {
            breaks += 1;
            if bytes.get(index + 1) == Some(&b'\n') {
                index += 1;
            }
        } else if bytes[index] == b'\n' {
            breaks += 1;
        }
        index += 1;
    }
    (content, breaks + 1)
}

fn markdown_file_link(filename: &str, relative_path: &str) -> String {
    let mut escaped_label = String::with_capacity(filename.len());
    for character in filename.chars() {
        match character {
            '\\' => escaped_label.push_str("\\\\"),
            '[' => escaped_label.push_str("\\["),
            ']' => escaped_label.push_str("\\]"),
            '\n' => escaped_label.push_str("\\n"),
            '\r' => escaped_label.push_str("\\r"),
            '\t' => escaped_label.push_str("\\t"),
            character if character.is_control() => {
                use fmt::Write as _;
                write!(&mut escaped_label, "\\u{{{:X}}}", u32::from(character))
                    .expect("writing to a String cannot fail");
            }
            character => escaped_label.push(character),
        }
    }
    let destination = percent_encode_path(relative_path);
    format!("[{escaped_label}]({destination})")
}

fn percent_encode_path(path: &str) -> String {
    let mut encoded = String::with_capacity(path.len());
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/') {
            encoded.push(char::from(byte));
        } else {
            use fmt::Write as _;
            write!(&mut encoded, "%{byte:02X}").expect("writing to a String cannot fail");
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn context(text: &str, line: usize) -> CopyContext {
        CopyContext {
            selected_text: text.to_owned(),
            start_line: line,
            start_column: 1,
            filename: "main.go".to_owned(),
            relative_path: "cmd/main.go".to_owned(),
            absolute_path: Some(PathBuf::from("/repo/cmd/main.go")),
            language: Some("Go".to_owned()),
            outline_symbol: None,
            document_text: None,
        }
    }

    #[test]
    fn formats_a_symbol() {
        assert_eq!(
            format_selection_with_kind(&context("Run", 12), Some(SymbolKind::Function)),
            "[main.go](cmd/main.go) 中的 function Run"
        );
    }

    #[test]
    fn formats_plain_single_line_text() {
        assert_eq!(
            format_selection_with_kind(&context("count", 12), None),
            "[main.go](cmd/main.go) 第 12 行的 count"
        );
    }

    #[test]
    fn formats_multiline_ranges_before_symbols() {
        assert_eq!(
            format_selection_with_kind(
                &context("func Run() {\n\twork()\n}", 12),
                Some(SymbolKind::Function)
            ),
            "[main.go](cmd/main.go) 第 12 到第 14 行的内容"
        );
    }

    #[test]
    fn trailing_line_ending_does_not_add_an_empty_line() {
        assert_eq!(
            format_selection_with_kind(&context("alpha\n", 8), None),
            "[main.go](cmd/main.go) 第 8 行的 alpha"
        );
        assert_eq!(
            format_selection_with_kind(&context("alpha\r\nbeta\r\n", 8), None),
            "[main.go](cmd/main.go) 第 8 到第 9 行的内容"
        );
        assert_eq!(
            format_selection_with_kind(&context("alpha\rbeta", 8), None),
            "[main.go](cmd/main.go) 第 8 到第 9 行的内容"
        );
        assert_eq!(
            format_selection_with_kind(&context("alpha\n\n", 8), None),
            "[main.go](cmd/main.go) 第 8 到第 9 行的内容"
        );
    }

    #[test]
    fn escapes_markdown_filename_and_destination() {
        let mut context = context("value", 2);
        context.filename = "a[1].go".to_owned();
        context.relative_path = "dir 空/a[1].go".to_owned();
        assert_eq!(
            format_selection_with_kind(&context, None),
            "[a\\[1\\].go](dir%20%E7%A9%BA/a%5B1%5D.go) 第 2 行的 value"
        );
    }

    #[test]
    fn escapes_control_characters_in_the_link_label() {
        let mut context = context("value", 2);
        context.filename = "odd\nname\t.go".to_owned();
        context.relative_path = "odd\nname\t.go".to_owned();
        assert_eq!(
            format_selection_with_kind(&context, None),
            "[odd\\nname\\t.go](odd%0Aname%09.go) 第 2 行的 value"
        );
    }

    #[test]
    fn saturates_an_impossibly_large_end_line() {
        assert_eq!(
            format_selection_with_kind(&context("a\nb", usize::MAX), None),
            format!(
                "[main.go](cmd/main.go) 第 {} 到第 {} 行的内容",
                usize::MAX,
                usize::MAX
            )
        );
    }
}
