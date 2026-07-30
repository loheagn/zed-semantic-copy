const path = require("node:path");
const { spawn } = require("node:child_process");
const vscode = require("vscode");

const { buildCopyContext, removeFinalLineEnding } = require("./context");

function binaryPath(extensionPath) {
  const filename = process.platform === "win32"
    ? "zed-semantic-copy.exe"
    : "zed-semantic-copy";
  return path.join(extensionPath, "bin", filename);
}

function formatWithCli(executable, context) {
  return new Promise((resolve, reject) => {
    const child = spawn(executable, ["copy", "--stdout", "--stdin-json"], {
      stdio: ["pipe", "pipe", "pipe"],
      windowsHide: true,
    });
    const stdout = [];
    const stderr = [];
    let spawnError;

    child.stdout.on("data", (chunk) => stdout.push(chunk));
    child.stderr.on("data", (chunk) => stderr.push(chunk));
    child.on("error", (error) => {
      spawnError = error;
    });
    child.stdin.on("error", (error) => {
      if (error.code !== "EPIPE") {
        spawnError = error;
      }
    });
    child.on("close", (code) => {
      if (spawnError) {
        reject(spawnError);
        return;
      }
      if (code !== 0) {
        const details = Buffer.concat(stderr).toString("utf8").trim();
        reject(new Error(details || `semantic copy exited with status ${code}`));
        return;
      }
      resolve(removeFinalLineEnding(Buffer.concat(stdout).toString("utf8")));
    });

    child.stdin.end(JSON.stringify(context));
  });
}

function activate(extensionContext) {
  const command = vscode.commands.registerCommand(
    "semanticCopy.copySelection",
    async () => {
      const editor = vscode.window.activeTextEditor;
      if (!editor) {
        vscode.window.showWarningMessage("Semantic Copy: no active text editor.");
        return;
      }

      const selectedText = editor.document.getText(editor.selection);
      if (selectedText.trim().length === 0) {
        vscode.window.showWarningMessage("Semantic Copy: select some text first.");
        return;
      }

      try {
        const context = buildCopyContext(editor, vscode.workspace.asRelativePath);
        const formatted = await formatWithCli(
          binaryPath(extensionContext.extensionPath),
          context,
        );
        await vscode.env.clipboard.writeText(formatted);
        vscode.window.setStatusBarMessage("Semantic reference copied", 2000);
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        vscode.window.showErrorMessage(`Semantic Copy failed: ${message}`);
      }
    },
  );

  extensionContext.subscriptions.push(command);
}

function deactivate() {}

module.exports = { activate, deactivate, formatWithCli };
