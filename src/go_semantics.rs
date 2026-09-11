use tree_sitter::Node;

use crate::{context::CopyContext, formatter::SymbolKind, semantics::classify_selection};

pub fn classify(context: &CopyContext) -> Option<SymbolKind> {
    if !is_go_context(context) {
        return None;
    }

    classify_selection(context, &tree_sitter_go::LANGUAGE.into(), classify_node)
}

fn is_go_context(context: &CopyContext) -> bool {
    context
        .language
        .as_deref()
        .is_some_and(|language| language.eq_ignore_ascii_case("go"))
        || context.relative_path.ends_with(".go")
}

fn classify_node(node: Node<'_>) -> Option<SymbolKind> {
    match node.kind() {
        "type_identifier" => classify_type_identifier(node),
        "package_identifier" => Some(SymbolKind::Package),
        "label_name" => Some(SymbolKind::Label),
        "field_identifier" => classify_field_identifier(node),
        "identifier" => classify_identifier(node),
        _ => None,
    }
}

fn classify_identifier(node: Node<'_>) -> Option<SymbolKind> {
    let parent = node.parent()?;
    match parent.kind() {
        "function_declaration" if is_field(parent, node, "name") => Some(SymbolKind::Function),
        "const_spec" if is_field(parent, node, "name") => Some(SymbolKind::Constant),
        "call_expression" if is_field(parent, node, "function") => Some(SymbolKind::Function),
        _ => None,
    }
}

fn classify_field_identifier(node: Node<'_>) -> Option<SymbolKind> {
    let parent = node.parent()?;
    match parent.kind() {
        "method_declaration" | "method_elem" if is_field(parent, node, "name") => {
            Some(SymbolKind::Method)
        }
        "field_declaration" if is_field(parent, node, "name") => Some(SymbolKind::Field),
        _ => None,
    }
}

fn classify_type_identifier(node: Node<'_>) -> Option<SymbolKind> {
    if let Some(parent) = node.parent()
        && matches!(parent.kind(), "type_spec" | "type_alias")
        && is_field(parent, node, "name")
    {
        return declared_type_kind(parent);
    }

    Some(SymbolKind::Type)
}

fn declared_type_kind(declaration: Node<'_>) -> Option<SymbolKind> {
    let declared_type = declaration.child_by_field_name("type")?;
    Some(match declared_type.kind() {
        "struct_type" => SymbolKind::Struct,
        "interface_type" => SymbolKind::Interface,
        _ => SymbolKind::Type,
    })
}

fn is_field(parent: Node<'_>, child: Node<'_>, expected: &str) -> bool {
    (0..parent.child_count()).any(|index| {
        parent.child(index).is_some_and(|candidate| {
            candidate.id() == child.id()
                && parent.field_name_for_child(index as u32) == Some(expected)
        })
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    const SOURCE: &str = r#"package main

import "fmt"

const DefaultCount = 3

type Greeter struct {
	Name string
}

type Runner interface {
	Run()
}

type Alias = string

func (Greeter) Hello(name string) string {
	message := "你好" + name
	return message
}

func helper(value int) int {
	return value + 1
}

func main() {
	g := Greeter{}
	result := g.Hello("world")
	fmt.Println(result)
	_ = helper(result)
}
"#;

    fn classify_occurrence(needle: &str, occurrence: usize) -> Option<SymbolKind> {
        let directory = tempdir().unwrap();
        let path = directory.path().join("main.go");
        fs::write(&path, SOURCE).unwrap();
        let byte = SOURCE
            .match_indices(needle)
            .nth(occurrence)
            .map(|(index, _)| index)
            .unwrap();
        let prefix = &SOURCE[..byte];
        let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
        let line_start = prefix.rfind('\n').map_or(0, |index| index + 1);
        let column = byte - line_start + 1;
        let context = CopyContext {
            selected_text: needle.to_owned(),
            start_line: line,
            start_column: column,
            filename: "main.go".to_owned(),
            relative_path: "main.go".to_owned(),
            absolute_path: Some(path),
            language: Some("Go".to_owned()),
            outline_symbol: None,
            document_text: None,
        };
        classify(&context)
    }

    #[test]
    fn classifies_go_declarations() {
        assert_eq!(
            classify_occurrence("DefaultCount", 0),
            Some(SymbolKind::Constant)
        );
        assert_eq!(classify_occurrence("Greeter", 0), Some(SymbolKind::Struct));
        assert_eq!(
            classify_occurrence("Runner", 0),
            Some(SymbolKind::Interface)
        );
        assert_eq!(classify_occurrence("Alias", 0), Some(SymbolKind::Type));
        assert_eq!(classify_occurrence("Hello", 0), Some(SymbolKind::Method));
        assert_eq!(classify_occurrence("helper", 0), Some(SymbolKind::Function));
        assert_eq!(classify_occurrence("Name", 0), Some(SymbolKind::Field));
    }

    #[test]
    fn classifies_calls_and_type_references() {
        assert_eq!(classify_occurrence("Greeter", 2), Some(SymbolKind::Type));
        assert_eq!(classify_occurrence("helper", 1), Some(SymbolKind::Function));
    }

    #[test]
    fn ambiguous_selectors_fall_back_to_plain_text() {
        assert_eq!(classify_occurrence("Hello", 1), None);
        assert_eq!(classify_occurrence("Println", 0), None);
    }

    #[test]
    fn ordinary_variables_are_not_symbols() {
        assert_eq!(classify_occurrence("message", 0), None);
        assert_eq!(classify_occurrence("result", 0), None);
        assert_eq!(classify_occurrence("name", 0), None);
    }

    #[test]
    fn mismatched_unsaved_source_falls_back() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("main.go");
        fs::write(&path, SOURCE).unwrap();
        let context = CopyContext {
            selected_text: "not-on-disk".to_owned(),
            start_line: 1,
            start_column: 1,
            filename: "main.go".to_owned(),
            relative_path: "main.go".to_owned(),
            absolute_path: Some(path),
            language: Some("Go".to_owned()),
            outline_symbol: None,
            document_text: None,
        };
        assert_eq!(classify(&context), None);
    }

    #[test]
    fn byte_columns_handle_multibyte_text_before_a_symbol() {
        let source = "package main\nfunc main() { _ = \"你好\"; helper() }\nfunc helper() {}\n";
        let directory = tempdir().unwrap();
        let path = directory.path().join("main.go");
        fs::write(&path, source).unwrap();
        let selected_byte = source.find("helper()").unwrap();
        let line_start = source[..selected_byte].rfind('\n').unwrap() + 1;
        let context = CopyContext {
            selected_text: "helper".to_owned(),
            start_line: 2,
            start_column: selected_byte - line_start + 1,
            filename: "main.go".to_owned(),
            relative_path: "main.go".to_owned(),
            absolute_path: Some(path),
            language: Some("Go".to_owned()),
            outline_symbol: None,
            document_text: Some(source.to_owned()),
        };
        assert_eq!(classify(&context), Some(SymbolKind::Function));
    }

    #[test]
    fn classifies_unsaved_document_text_without_a_file() {
        let context = CopyContext {
            selected_text: "helper".to_owned(),
            start_line: 3,
            start_column: 6,
            filename: "main.go".to_owned(),
            relative_path: "main.go".to_owned(),
            absolute_path: None,
            language: Some("go".to_owned()),
            outline_symbol: None,
            document_text: Some("package main\nfunc main() {}\nfunc helper() {}\n".to_owned()),
        };

        assert_eq!(classify(&context), Some(SymbolKind::Function));
    }
}
