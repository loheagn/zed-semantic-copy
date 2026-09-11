# Semantic Copy for Zed and VS Code

把 Zed 或 Visual Studio Code 中的选区转换成适合粘贴到 Codex Prompt 的语义引用，并写入系统剪贴板。

```text
[service.go](internal/service.go) 中的 function NewService
[models.py](app/models.py) 中的 class User
[service.go](internal/service.go) 第 42 行的 request
[service.go](internal/service.go) 第 42 到第 57 行的内容
```

## 编辑器集成

Zed 使用 Task + CLI；VS Code 使用一个轻量扩展读取选区，再调用同一套 Rust 格式化与 Go/Python 语义识别逻辑。两边生成的引用格式一致。

### 为什么 Zed 采用 Task + CLI

截至 Zed 1.11.3，公开的 WASM Extension API 不能读取活动编辑器选区、注册任意 Editor Action 或写系统剪贴板。因此当前版本使用 Zed 全局 Task 把 `ZED_SELECTED_TEXT`、文件路径和起始位置传给一个小型 Rust CLI，再由 CLI 调用 `/usr/bin/pbcopy`。

这不是 Extension Gallery 中可一键安装的扩展，但日常使用仍是一个编辑器快捷键。相关限制可参考 [Zed Tasks](https://zed.dev/docs/tasks) 和 [Extension Capabilities](https://zed.dev/docs/extensions/capabilities)。

## 安装 Zed 集成

要求：

- macOS
- Zed 1.11.3 或更新版本
- Rust 1.85 或更新版本（仅从源码安装时需要）

在仓库目录运行：

```bash
cargo run --release -- install
```

安装器会：

1. 将当前二进制复制到 `~/.local/bin/zed-semantic-copy`。
2. 在 `~/.config/zed/tasks.json` 中追加带受管标记的隐藏 Task。
3. 在 `~/.config/zed/keymap.json` 中追加 `space y s` 和 `Shift+Command+C` 快捷键。

已有 JSONC 注释和配置会被保留；重复安装不会产生重复条目。安装前可预览：

```bash
cargo run --release -- install --dry-run
```

如果 `space y s` 已被占用，可指定其他 Vim 按键；`Shift+Command+C` 会始终同时安装：

```bash
cargo run --release -- install --keybinding "space y c"
```

## 安装 VS Code 集成

要求：

- VS Code 1.85 或更新版本
- Rust 1.85 或更新版本
- Node.js 20 或更新版本及 npm（仅构建扩展时需要）

扩展包内含当前操作系统和 CPU 架构的 Rust helper，因此请在最终使用 VS Code 的同类平台上构建：

```bash
cd editors/vscode
npm install
npm run package
code --install-extension semantic-copy-0.1.0.vsix
```

更新代码后重复运行 `npm run package` 和 `code --install-extension ... --force` 即可覆盖安装。

## 使用

在编辑器中选中文本，然后运行 `Semantic Copy: Copy Selection as Reference`，也可以使用以下快捷键：

- Zed：`space y s` 或 `Shift+Command+C`
- VS Code：macOS 上为 `Shift+Command+C`，Windows/Linux 上为 `Ctrl+Shift+C`

引用规则：

- Go 函数、方法、结构体、接口、类型、常量等能从语法明确判断的符号，输出语义格式。
- Python 函数、类和类中方法的声明名称，输出语义格式，包括异步函数和带装饰器的声明；`.py` 和 `.pyi` 文件均支持。
- 普通变量或无法可靠判断的标识符，输出单行格式；Python 中的参数、属性访问和调用名称也按此处理。
- 实际覆盖多行内容时，输出起止行范围；行号从 1 开始。
- 三种格式都带相对工作区的 Markdown 文件链接。

Zed Task 不会保存文件。Zed 的 Go/Python 符号判断基于磁盘内容：选区与磁盘位置不匹配时会安全降级；若未保存的改动只改变了选区周边语法而没有改变选区位置，分类可能仍基于旧语法。需要绝对准确时请先保存文件。VS Code 扩展会传入当前内存中的完整文档，因此未保存内容也能准确判断。

CLI 会为 `pbcopy` 显式使用 UTF-8 编码，避免从 Finder/Dock 启动 Zed 时因进程缺少 locale 而丢失中文引用。

也可以从 Zed 的 Task Picker 运行 `zed-semantic-copy: copy selection`。CLI 默认读取 Zed 注入的 `ZED_*` 环境变量；调试时可用 `copy --stdout` 只打印结果而不改剪贴板。VS Code 中还可以从命令面板或编辑器右键菜单运行。

## 当前边界

- v1 识别 Go 和 Python；其他语言仍可得到准确的文件、行号和多行范围，但单行标识符按普通文本处理。
- `pkg.Func`、`obj.Method`、`client.get`、字段/常量引用等仅靠语法无法可靠区分的 selector 或属性访问会保守降级为普通单行格式；声明位置仍会标注准确类型。
- Zed Task API 只提供最新的一个选区；VS Code 扩展使用主选区。两者都不会合并多光标选区。
- Zed 集成依赖 macOS 的 `pbcopy`；VS Code 扩展使用编辑器剪贴板 API，但扩展中的原生 helper 需要在目标平台构建。
- 选择整行产生的末尾换行不额外增加结束行，例如 `foo\n` 仍描述为一行。
- 路径中的空格、括号、`#`、Unicode 等会按 Markdown URL 规则编码。

## 卸载

以下命令卸载 Zed 集成：

```bash
~/.local/bin/zed-semantic-copy uninstall
```

卸载器只删除自己的受管配置块，不覆盖安装后产生的其他 Zed 配置变更。首次修改现有配置时创建的 `*.zed-semantic-copy.bak` 安全备份会保留。只预览操作：

```bash
~/.local/bin/zed-semantic-copy uninstall --dry-run
```

VS Code 集成可从扩展面板卸载，或运行：

```bash
code --uninstall-extension zed-semantic-copy.semantic-copy
```

## 手动配置

若不想运行安装器，可参考：

- [`examples/tasks.json`](examples/tasks.json)
- [`examples/keymap.json`](examples/keymap.json)

把 Task 中的 `command` 替换成实际二进制绝对路径。

## 开发

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features

cd editors/vscode
npm test
npm run package
```
