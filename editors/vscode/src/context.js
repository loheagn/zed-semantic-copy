const path = require("node:path");

function utf8Column(lineText, utf16Character) {
  return Buffer.byteLength(lineText.slice(0, utf16Character), "utf8") + 1;
}

function normalizeRelativePath(relativePath) {
  return relativePath.replaceAll("\\", "/");
}

function buildCopyContext(editor, asRelativePath) {
  const { document, selection } = editor;
  const selectedText = document.getText(selection);
  const filename = path.basename(document.fileName || document.uri.path);
  const relativePath = normalizeRelativePath(asRelativePath(document.uri, false));
  const context = {
    selected_text: selectedText,
    start_line: selection.start.line + 1,
    start_column: utf8Column(
      document.lineAt(selection.start.line).text,
      selection.start.character,
    ),
    filename,
    relative_path: relativePath || filename,
    language: document.languageId,
    document_text: document.getText(),
  };

  if (document.uri.scheme === "file") {
    context.absolute_path = document.uri.fsPath;
  }

  return context;
}

function removeFinalLineEnding(value) {
  return value.replace(/\r?\n$/, "");
}

module.exports = {
  buildCopyContext,
  normalizeRelativePath,
  removeFinalLineEnding,
  utf8Column,
};
