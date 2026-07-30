const fs = require("node:fs");
const path = require("node:path");
const { spawnSync } = require("node:child_process");

const extensionRoot = path.resolve(__dirname, "..");
const repositoryRoot = path.resolve(extensionRoot, "..", "..");
const targetRoot = process.env.CARGO_TARGET_DIR
  ? path.resolve(repositoryRoot, process.env.CARGO_TARGET_DIR)
  : path.join(repositoryRoot, "target");
const filename = process.platform === "win32"
  ? "zed-semantic-copy.exe"
  : "zed-semantic-copy";

const result = spawnSync("cargo", ["build", "--release", "--locked"], {
  cwd: repositoryRoot,
  stdio: "inherit",
});
if (result.error) {
  throw result.error;
}
if (result.status !== 0) {
  process.exit(result.status ?? 1);
}

const source = path.join(targetRoot, "release", filename);
const destinationDirectory = path.join(extensionRoot, "bin");
const destination = path.join(destinationDirectory, filename);
fs.mkdirSync(destinationDirectory, { recursive: true });
fs.copyFileSync(source, destination);
if (process.platform !== "win32") {
  fs.chmodSync(destination, 0o755);
}
console.log(`Bundled ${destination}`);
