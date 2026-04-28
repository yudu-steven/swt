# swt

[中文] | [English](README_EN.md)

> 浏览、搜索、恢复和删除 AI 编程会话 — OpenCode、Claude Code、Codex。
> **单文件，零配置。**

```
  ╔══════════════════════════════════════════════╗
  ║                 >>  swt  <<                  ║
  ║           AI Coding Session Manager          ║
  ╚══════════════════════════════════════════════╝
```

## 功能

- **3 个 Provider**: OpenCode、Claude Code、Codex
- **交互模式**: 直接输入 `swt` — 方向键浏览选择
- **列出会话**: `swt ls` — 带 Provider 图标、时间、项目名
- **查看对话**: `swt cat <id>` — 完整消息时间线，角色着色
- **恢复会话**: `swt res <id>` — 复制恢复命令到剪贴板
- **终端启动**: `swt res <id> --launch` (Windows)
- **删除会话**: `swt rm <id>`
- **搜索**: `swt ls --search 关键词`
- **零配置**: 自动检测多个候选路径中的 OpenCode/Claude/Codex 数据

## 添加到 PATH（推荐）

将 `swt.exe` 所在目录加入系统 PATH，任何终端直接 `swt` 即可使用：

```powershell
[Environment]::SetEnvironmentVariable("Path", $env:Path + ";你的安装目录", [EnvironmentVariableTarget]::User)
```

把 `你的安装目录` 改成 `swt.exe` 实际所在路径（如 `D:\tools`）。新开终端验证：

```powershell
swt --version
```

## 快速开始

```powershell
# 交互模式（推荐）
swt

# 列出所有会话
swt ls

# 只看 OpenCode
swt ls opencode

# 搜索
swt ls --search 关键词

# 查看对话
swt cat ses_231d

# 恢复 — 复制命令到剪贴板
swt res ses_231d

# 恢复 — 在终端中打开
swt res ses_231d --launch

# 删除
swt rm ses_231d --provider opencode

# 查看数据路径
swt info
```

## 安装

### Scoop (Windows)

```powershell
scoop bucket add yudu-steven https://github.com/yudu-steven/scoop-bucket
scoop install swt
```

### Homebrew (macOS / Linux)

```bash
brew tap yudu-steven/swt
brew install swt
```

### 通过 Cargo 安装

```bash
cargo install --git https://github.com/yudu-steven/swt
```

### 从源码构建

```bash
git clone https://github.com/yudu-steven/swt
cd swt
cargo build --release
# 产物: target/release/swt (或 swt.exe)
```

## 工作原理

`swt` 扫描 AI 编程工具的本地会话数据：

| Provider | 数据路径 (Windows) |
|----------|-------------------|
| **OpenCode** | `%USERPROFILE%\.local\share\opencode\opencode.db` (SQLite) 或 `storage/` (JSON) |
| **Claude Code** | `%USERPROFILE%\.claude\projects\*.jsonl` |
| **Codex** | `%USERPROFILE%\.codex\sessions\*.jsonl` |

多种候选路径都会被扫描（USERPROFILE、HOME、系统 home 目录），即使在沙箱化 shell 中也能正常工作。

**默认只读** — swt 不会修改你的会话文件，除非你主动执行 `swt rm`。

## 为什么选择 swt？

| | cc-switch | swt |
|---|---|---|
| **体积** | ~15-30 MB (Tauri 桌面应用) | **~4 MB** (单个二进制) |
| **界面** | 完整 React UI | 终端原生 |
| **Provider 数** | 6 个 | 3 个 (OpenCode/Claude/Codex) |
| **配置** | SQLite 数据库 | 无需配置 |
| **使用方式** | 桌面应用 + 托盘 | CLI + 交互式 TUI |

## 许可证

MIT
