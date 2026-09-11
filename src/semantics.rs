use std::{borrow::Cow, fs};

use tree_sitter::{Language, Node, Parser};

use crate::{context::CopyContext, formatter::SymbolKind};

pub(crate) fn classify_selection(
    context: &CopyContext,
    language: &Language,
    classify_node: impl FnOnce(Node<'_>) -> Option<SymbolKind>,
) -> Option<SymbolKind> {
    if context.selected_text.is_empty() || context.selected_text.contains(['\n', '\r']) {
        return None;
    }

    let source = match context.document_text.as_ref() {
        Some(document_text) => Cow::Borrowed(document_text.as_bytes()),
        None => Cow::Owned(fs::read(context.absolute_path.as_deref()?).ok()?),
    };
    let start = byte_offset(&source, context.start_line, context.start_column)?;
    let end = start.checked_add(context.selected_text.len())?;
    if source.get(start..end)? != context.selected_text.as_bytes() {
        return None;
    }

    let mut parser = Parser::new();
    parser.set_language(language).ok()?;
    let tree = parser.parse(&source, None)?;
    let node = tree
        .root_node()
        .named_descendant_for_byte_range(start, end)?;
    if node.start_byte() != start || node.end_byte() != end {
        return None;
    }

    classify_node(node)
}

fn byte_offset(source: &[u8], one_based_line: usize, one_based_column: usize) -> Option<usize> {
    let mut line_start = 0;
    for _ in 0..one_based_line.checked_sub(1)? {
        let newline = source
            .get(line_start..)?
            .iter()
            .position(|byte| *byte == b'\n')?;
        line_start += newline + 1;
    }

    let offset = line_start.checked_add(one_based_column.checked_sub(1)?)?;
    let line_end = source
        .get(line_start..)?
        .iter()
        .position(|byte| *byte == b'\n')
        .map_or(source.len(), |newline| line_start + newline);
    (offset <= line_end).then_some(offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_offset_rejects_invalid_positions() {
        assert_eq!(byte_offset(b"a\nb", 0, 1), None);
        assert_eq!(byte_offset(b"a\nb", 1, 0), None);
        assert_eq!(byte_offset(b"a\nb", 3, 1), None);
        assert_eq!(byte_offset(b"a\nb", 1, 3), None);
        assert_eq!(byte_offset(b"a\nb", 2, 2), Some(3));
        assert_eq!(byte_offset(b"a\nb", 2, usize::MAX), None);
    }
}
