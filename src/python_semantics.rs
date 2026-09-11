use std::path::Path;

use tree_sitter::Node;

use crate::{context::CopyContext, formatter::SymbolKind, semantics::classify_selection};

pub fn classify(context: &CopyContext) -> Option<SymbolKind> {
    if !is_python_context(context) {
        return None;
    }

    classify_selection(context, &tree_sitter_python::LANGUAGE.into(), classify_node)
}

fn is_python_context(context: &CopyContext) -> bool {
    context
        .language
        .as_deref()
        .is_some_and(|language| language.eq_ignore_ascii_case("python"))
        || Path::new(&context.relative_path)
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                extension.eq_ignore_ascii_case("py") || extension.eq_ignore_ascii_case("pyi")
            })
}

fn classify_node(node: Node<'_>) -> Option<SymbolKind> {
    if node.kind() != "identifier" {
        return None;
    }

    let declaration = node.parent()?;
    if declaration.child_by_field_name("name") != Some(node) || !has_valid_header(declaration) {
        return None;
    }

    match declaration.kind() {
        "class_definition" => Some(SymbolKind::Class),
        "function_definition" => Some(function_kind(declaration)),
        _ => None,
    }
}

fn has_valid_header(declaration: Node<'_>) -> bool {
    // Recovery errors from an unfinished body can also be direct children
    // of the declaration. Only validate through the header's colon.
    let mut cursor = declaration.walk();
    for child in declaration.children(&mut cursor) {
        if child.has_error() || child.is_missing() {
            return false;
        }
        if child.kind() == ":" {
            return true;
        }
    }
    false
}

fn function_kind(declaration: Node<'_>) -> SymbolKind {
    // Decorators and control-flow blocks do not introduce a Python scope.
    // Stop at the nearest class/function so a function nested in a method
    // remains a function, while a method of a nested class remains a method.
    let mut ancestor = declaration.parent();
    while let Some(node) = ancestor {
        match node.kind() {
            "class_definition" => return SymbolKind::Method,
            "function_definition" => return SymbolKind::Function,
            _ => ancestor = node.parent(),
        }
    }
    SymbolKind::Function
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    const SOURCE: &str = r#"import asyncio

DEFAULT_COUNT = 3

@decorate
class Greeter(Base):
    salutation: str = "hello"

    def __init__(self, name: str):
        self.name = name

    @trace
    async def greet(self):
        def nested():
            return self.name
        return nested()

    @staticmethod
    def helper(value):
        return value

    @classmethod
    def create(cls):
        return cls("world")

    @property
    def label(self):
        return self.name

    if enabled:
        def conditional(self):
            pass

@trace
def run():
    result = Greeter("world")
    return result.greet()

async def fetch():
    return await asyncio.sleep(0)

def factory():
    class Inner:
        def invoke(self):
            pass
    return Inner

run()
"#;

    fn context_at(source: &str, needle: &str, occurrence: usize) -> CopyContext {
        let byte = source.match_indices(needle).nth(occurrence).unwrap().0;
        let prefix = &source[..byte];
        CopyContext {
            selected_text: needle.to_owned(),
            start_line: prefix.bytes().filter(|byte| *byte == b'\n').count() + 1,
            start_column: byte - prefix.rfind('\n').map_or(0, |index| index + 1) + 1,
            filename: "main.py".to_owned(),
            relative_path: "src/main.py".to_owned(),
            absolute_path: None,
            language: Some("Python".to_owned()),
            outline_symbol: None,
            document_text: Some(source.to_owned()),
        }
    }

    #[test]
    fn classifies_declarations_including_decorators_and_async() {
        for (name, kind) in [
            ("Greeter", SymbolKind::Class),
            ("__init__", SymbolKind::Method),
            ("greet", SymbolKind::Method),
            ("helper", SymbolKind::Method),
            ("create", SymbolKind::Method),
            ("label", SymbolKind::Method),
            ("conditional", SymbolKind::Method),
            ("run", SymbolKind::Function),
            ("fetch", SymbolKind::Function),
        ] {
            assert_eq!(classify(&context_at(SOURCE, name, 0)), Some(kind), "{name}");
        }
    }

    #[test]
    fn distinguishes_nested_functions_and_methods_of_nested_classes() {
        for (name, kind) in [
            ("nested", SymbolKind::Function),
            ("factory", SymbolKind::Function),
            ("Inner", SymbolKind::Class),
            ("invoke", SymbolKind::Method),
        ] {
            assert_eq!(classify(&context_at(SOURCE, name, 0)), Some(kind), "{name}");
        }
    }

    #[test]
    fn ambiguous_references_and_ordinary_identifiers_fall_back() {
        for (name, occurrence) in [
            ("asyncio", 0),
            ("DEFAULT_COUNT", 0),
            ("Base", 0),
            ("salutation", 0),
            ("self", 0),
            ("name", 0),
            ("str", 0),
            ("result", 0),
            ("Greeter", 1),
            ("run", 1),
            ("greet", 1),
            ("nested", 1),
            ("trace", 0),
        ] {
            assert_eq!(
                classify(&context_at(SOURCE, name, occurrence)),
                None,
                "{name}"
            );
        }
    }

    #[test]
    fn detects_python_language_and_file_extensions() {
        let mut context = context_at(SOURCE, "run", 0);
        context.relative_path = "Untitled-1".to_owned();
        for language in ["Python", "python", "PYTHON"] {
            context.language = Some(language.to_owned());
            assert_eq!(classify(&context), Some(SymbolKind::Function));
        }
        context.language = None;
        for path in ["src/main.py", "src/types.pyi", "src/MAIN.PY"] {
            context.relative_path = path.to_owned();
            assert_eq!(classify(&context), Some(SymbolKind::Function));
        }
        context.relative_path = "notes.txt".to_owned();
        assert_eq!(classify(&context), None);
    }

    #[test]
    fn reads_disk_and_prefers_unsaved_document_text() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("main.py");
        let source = "def run():\n    pass\n";
        let mut context = context_at(source, "run", 0);
        context.absolute_path = Some(path.clone());
        context.document_text = None;
        fs::write(&path, source).unwrap();
        assert_eq!(classify(&context), Some(SymbolKind::Function));

        // The same text at the same position on disk is an ordinary variable.
        fs::write(&path, "# comment\n    run = 1\n").unwrap();
        context = context_at("# comment\ndef run(): pass\n", "run", 0);
        context.absolute_path = Some(path);
        assert_eq!(classify(&context), Some(SymbolKind::Function));
        context.document_text = None;
        assert_eq!(classify(&context), None);
    }

    #[test]
    fn stale_or_unavailable_source_falls_back() {
        let mut context = context_at(SOURCE, "run", 0);
        context.selected_text = "renamed".to_owned();
        assert_eq!(classify(&context), None);
        context.selected_text = "run".to_owned();
        context.document_text = None;
        assert_eq!(classify(&context), None);
        let directory = tempdir().unwrap();
        context.absolute_path = Some(directory.path().join("missing.py"));
        assert_eq!(classify(&context), None);
    }

    #[test]
    fn requires_an_exact_identifier_selection() {
        for needle in [
            "ru",
            "run()",
            "def run",
            "run():\n    result",
            "run():",
            " run",
        ] {
            assert_eq!(classify(&context_at(SOURCE, needle, 0)), None, "{needle}");
        }
        let source = "# run\nvalue = 'run'\ndef run(): pass\n";
        assert_eq!(classify(&context_at(source, "run", 0)), None);
        assert_eq!(classify(&context_at(source, "run", 1)), None);
    }

    #[test]
    fn supports_unicode_identifiers_and_crlf() {
        let source = "class 问候:\r\n\tasync def 打招呼(self):\r\n\t\tpass\r\n";
        assert_eq!(
            classify(&context_at(source, "问候", 0)),
            Some(SymbolKind::Class)
        );
        assert_eq!(
            classify(&context_at(source, "打招呼", 0)),
            Some(SymbolKind::Method)
        );
    }

    #[test]
    fn supports_stub_definitions() {
        let source = "class Service:\n    def run(self) -> None: ...\n";
        let mut context = context_at(source, "run", 0);
        context.relative_path = "service.pyi".to_owned();
        context.language = None;
        assert_eq!(classify(&context), Some(SymbolKind::Method));
    }

    #[test]
    fn malformed_declarations_fall_back_without_hiding_unrelated_symbols() {
        for source in [
            "def broken(: pass\n",
            "class broken(: pass\n",
            // An unclosed delimiter may prevent recovery of any declaration.
            "def broken():\n    x = (\n",
        ] {
            assert_eq!(classify(&context_at(source, "broken", 0)), None);
        }
        let source = "broken = (\ndef valid(): pass\n";
        assert_eq!(
            classify(&context_at(source, "valid", 0)),
            Some(SymbolKind::Function)
        );
    }

    #[test]
    fn unfinished_bodies_do_not_hide_recoverable_declarations() {
        for (source, name, kind) in [
            ("def good():\n    x =\n", "good", SymbolKind::Function),
            (
                "class Service:\n    def broken(:\n",
                "Service",
                SymbolKind::Class,
            ),
            (
                "class Service:\n    async def run(self):\n        return )\n",
                "run",
                SymbolKind::Method,
            ),
        ] {
            assert_eq!(classify(&context_at(source, name, 0)), Some(kind), "{name}");
        }
    }
}
