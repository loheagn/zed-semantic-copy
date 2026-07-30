const assert = require("node:assert/strict");
const test = require("node:test");

const {
  buildCopyContext,
  normalizeRelativePath,
  removeFinalLineEnding,
  utf8Column,
} = require("../src/context");

test("converts VS Code UTF-16 positions to one-based UTF-8 columns", () => {
  assert.equal(utf8Column("你好Run", 2), 7);
  assert.equal(utf8Column("a😀Run", 3), 6);
});

test("builds a complete context including unsaved document text", () => {
  const documentText = "package main\nfunc 你好Run() {}\n";
  const document = {
    fileName: "/repo/cmd/main.go",
    languageId: "go",
    uri: {
      scheme: "file",
      fsPath: "/repo/cmd/main.go",
      path: "/repo/cmd/main.go",
    },
    getText(selection) {
      return selection ? "Run" : documentText;
    },
    lineAt() {
      return { text: "func 你好Run() {}" };
    },
  };
  const editor = {
    document,
    selection: { start: { line: 1, character: 7 } },
  };

  const context = buildCopyContext(editor, () => "cmd\\main.go");

  assert.deepEqual(context, {
    selected_text: "Run",
    start_line: 2,
    start_column: 12,
    filename: "main.go",
    relative_path: "cmd/main.go",
    absolute_path: "/repo/cmd/main.go",
    language: "go",
    document_text: documentText,
  });
});

test("normalizes paths and removes only the CLI line ending", () => {
  assert.equal(normalizeRelativePath("dir\\main.go"), "dir/main.go");
  assert.equal(removeFinalLineEnding("reference\n"), "reference");
  assert.equal(removeFinalLineEnding("reference"), "reference");
});
