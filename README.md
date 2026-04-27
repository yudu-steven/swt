# swt

> Browse, search, resume and delete AI coding sessions �?OpenCode, Claude Code, Codex.
> **One binary, zero config.**

```
  ╔══════════════════════════════════════════════╗
  �?              �? swt  �?                 �?  �?  AI Coding Session Manager                  �?  ╚══════════════════════════════════════════════╝
```

## Features

- **3 providers**: OpenCode, Claude Code, Codex
- **Interactive mode**: `swt` (no args) �?browse with arrow keys
- **List sessions**: `swt ls` �?with provider icons, timestamps, projects
- **View conversations**: `swt cat <id>` �?full message timeline with role colors
- **Resume sessions**: `swt res <id>` �?copies `cd dir && resume-command` to clipboard
- **Terminal launch**: `swt res <id> --launch` (Windows)
- **Delete sessions**: `swt rm <id>`
- **Search**: `swt ls --search keyword`
- **Zero config**: auto-detects OpenCode/Claude/Codex data paths across multiple candidates

## Quick Start

```powershell
# Interactive mode (recommended)
swt

# List all sessions
swt ls

# List only OpenCode sessions
swt ls opencode

# Search
swt ls --search keyword

# Show conversation
swt cat ses_231d

# Resume �?copy command to clipboard
swt res ses_231d

# Resume �?open in terminal
swt res ses_231d --launch

# Delete
swt rm ses_231d --provider opencode

# Check data paths
swt info
```

## Installation

### Download prebuilt binary

Go to [Releases](https://github.com/your-username/swt/releases) and download `swt.exe` (Windows) or `swt` (macOS/Linux).

### Install via Cargo

```bash
cargo install --git https://github.com/your-username/swt
```

### Build from source

```bash
git clone https://github.com/your-username/swt
cd swt
cargo build --release
# Binary at: target/release/swt (or swt.exe)
```

## How It Works

`swt` scans local session data from AI coding tools:

| Provider | Data Path (Windows) |
|----------|-------------------|
| **OpenCode** | `%USERPROFILE%\.local\share\opencode\opencode.db` (SQLite) or `storage/` (JSON) |
| **Claude Code** | `%USERPROFILE%\.claude\projects\*.jsonl` |
| **Codex** | `%USERPROFILE%\.codex\sessions\*.jsonl` |

Multiple candidate paths are scanned (USERPROFILE, HOME, system home dir), so it works
even in sandboxed shells.

**Read-only by default** �?swt never modifies your session files unless you explicitly run `swt rm`.

## Why swt?

| | cc-swt | swt |
|---|---|---|
| **Size** | ~15-30 MB (Tauri desktop app) | **~4 MB** (single binary) |
| **GUI** | Full React UI | Terminal-native |
| **Providers** | 6 (Claude/Codex/OpenCode/OpenClaw/Gemini/Hermes) | 3 (OpenCode/Claude/Codex) |
| **Config** | SQLite database | None required |
| **Usage** | Desktop app + tray | CLI + interactive TUI |

## License

MIT
