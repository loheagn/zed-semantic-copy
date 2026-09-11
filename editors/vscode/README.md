# Semantic Copy for VS Code

Copy the active editor selection as a semantic Markdown reference using the
same formatter and Go/Python symbol classifier as `zed-semantic-copy`.

The command is available as **Semantic Copy: Copy Selection as Reference**, in
the editor context menu, and through `Shift+Command+C` on macOS or
`Ctrl+Shift+C` on Windows/Linux.

This extension bundles a platform-native Rust helper. Build the VSIX on the
same operating system and CPU architecture where it will be installed.

```bash
npm install
npm run package
code --install-extension semantic-copy-0.1.0.vsix
```

After rebuilding, add `--force` to the install command to replace an existing
copy of the same version.

The VS Code API supplies the active selection and clipboard. The extension
also sends the current in-memory document text to the helper, so Go and Python
symbol classification remains accurate before the file is saved.

Python support covers declarations such as functions, decorated or async
functions, classes, and methods in `.py` and `.pyi` files. Local
variables, parameters, attributes, and arbitrary calls are intentionally copied
as plain line references.
