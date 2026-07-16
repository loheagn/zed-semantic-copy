# Zed Semantic Copy

把 Zed 中的选区转换成适合粘贴到 Codex Prompt 的语义引用，并写入 macOS 系统剪贴板。

```text
[service.go](internal/service.go) 中的 function NewService
[service.go](internal/service.go) 第 42 行的 request
[service.go](internal/service.go) 第 42 到第 57 行的内容
```

## 为什么采用 Task + CLI

截至 Zed 1.11.3，公开的 WASM Extension API 不能读取活动编辑器选区、注册任意 Editor Action 或写系统剪贴板。因此当前版本使用 Zed 全局 Task 把 `ZED_SELECTED_TEXT`、文件路径和起始位置传给一个小型 Rust CLI，再由 CLI 调用 `/usr/bin/pbcopy`。

这不是 Extension Gallery 中可一键安装的扩展，但日常使用仍是一个编辑器快捷键。相关限制可参考 [Zed Tasks](https://zed.dev/docs/tasks) 和 [Extension Capabilities](https://zed.dev/docs/extensions/capabilities)。

## 安装

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

## 使用

在 Zed 中选中文本，然后按 `space y s` 或 `Shift+Command+C`：

- Go 函数、方法、结构体、接口、类型、常量等能从语法明确判断的符号，输出语义格式。
- 普通变量或无法可靠判断的标识符，输出单行格式。
- 实际覆盖多行内容时，输出起止行范围；行号从 1 开始。
- 三种格式都带相对工作区的 Markdown 文件链接。

Task 不会保存文件。Go 符号判断基于磁盘内容：选区与磁盘位置不匹配时会安全降级；若未保存的改动只改变了选区周边语法而没有改变选区位置，分类可能仍基于旧语法。需要绝对准确时请先保存文件。

CLI 会为 `pbcopy` 显式使用 UTF-8 编码，避免从 Finder/Dock 启动 Zed 时因进程缺少 locale 而丢失中文引用。

也可以从 Zed 的 Task Picker 运行 `zed-semantic-copy: copy selection`。CLI 默认读取 Zed 注入的 `ZED_*` 环境变量；调试时可用 `copy --stdout` 只打印结果而不改剪贴板。

## 当前边界

- v1 优先识别 Go；其他语言仍可得到准确的文件、行号和多行范围，但单行标识符按普通文本处理。
- `pkg.Func`、`obj.Method`、字段/常量引用等仅靠语法无法可靠区分的 selector 会保守降级为普通单行格式；声明位置仍会标注准确类型。
- Zed Task API 只提供最新的一个选区，多光标的其他选区不会被合并。
- 仅支持 macOS 的 `pbcopy`。
- 选择整行产生的末尾换行不额外增加结束行，例如 `foo\n` 仍描述为一行。
- 路径中的空格、括号、`#`、Unicode 等会按 Markdown URL 规则编码。

## 卸载

```bash
~/.local/bin/zed-semantic-copy uninstall
```

卸载器只删除自己的受管配置块，不覆盖安装后产生的其他 Zed 配置变更。首次修改现有配置时创建的 `*.zed-semantic-copy.bak` 安全备份会保留。只预览操作：

```bash
~/.local/bin/zed-semantic-copy uninstall --dry-run
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
```
