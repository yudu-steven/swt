use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use chrono::{DateTime, FixedOffset};
use serde_json::Value;

pub const TITLE_MAX_CHARS: usize = 80;

// ── Platform path helpers ──

/// Return all candidate home directories.
/// On Windows this includes USERPROFILE, HOME, and the OS-provided home dir,
/// because sandboxed shells may have a different profile path.
pub fn candidate_home_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    // USERPROFILE takes priority on Windows
    if let Ok(up) = std::env::var("USERPROFILE") {
        if !up.is_empty() {
            dirs.push(PathBuf::from(up));
        }
    }
    // HOME may differ from USERPROFILE in some environments
    if let Ok(home) = std::env::var("HOME") {
        if !home.is_empty() {
            let p = PathBuf::from(home);
            if !dirs.contains(&p) {
                dirs.push(p);
            }
        }
    }
    // OS-provided home dir
    if let Some(home) = dirs::home_dir() {
        if !dirs.contains(&home) {
            dirs.push(home);
        }
    }
    dirs
}

/// Collect all candidate OpenCode base directories.
pub fn opencode_base_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    // Explicit override via env
    if let Ok(env) = std::env::var("CC_SWITCH_OPENCODE_DIR") {
        let p = PathBuf::from(env.trim());
        if !dirs.contains(&p) {
            dirs.push(p);
        }
    }

    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        if !xdg.is_empty() {
            let p = PathBuf::from(&xdg).join("opencode");
            if !dirs.contains(&p) {
                dirs.push(p);
            }
        }
    }

    // %LOCALAPPDATA%/opencode
    if let Some(local) = dirs::data_local_dir() {
        let p = local.join("opencode");
        if !dirs.contains(&p) {
            dirs.push(p);
        }
    }

    for home in candidate_home_dirs() {
        let p = home.join(".local").join("share").join("opencode");
        if !dirs.contains(&p) {
            dirs.push(p);
        }
    }

    dirs
}

/// Return the first existing OpenCode base dir (for display purposes)
pub fn opencode_base_dir() -> Option<PathBuf> {
    opencode_base_dirs().into_iter().find(|p| p.exists())
}

/// Collect all candidate Codex sessions directories.
pub fn codex_sessions_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Ok(env) = std::env::var("CC_SWITCH_CODEX_SESSIONS_DIR") {
        let p = PathBuf::from(env.trim());
        if !dirs.contains(&p) {
            dirs.push(p);
        }
    }

    if let Ok(codex_home) = std::env::var("CODEX_HOME") {
        let trimmed = codex_home.trim();
        if !trimmed.is_empty() {
            let p = PathBuf::from(trimmed).join("sessions");
            if !dirs.contains(&p) {
                dirs.push(p);
            }
        }
    }

    for home in candidate_home_dirs() {
        let p = home.join(".codex").join("sessions");
        if !dirs.contains(&p) {
            dirs.push(p);
        }
    }

    dirs
}

/// Return the first existing Codex sessions dir.
pub fn codex_sessions_dir() -> Option<PathBuf> {
    codex_sessions_dirs().into_iter().find(|p| p.exists())
}

/// Collect all candidate Claude Code projects directories.
pub fn claude_projects_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Ok(env) = std::env::var("CC_SWITCH_CLAUDE_PROJECTS_DIR") {
        let p = PathBuf::from(env.trim());
        if !dirs.contains(&p) {
            dirs.push(p);
        }
    }

    if let Ok(claude_config_dir) = std::env::var("CLAUDE_CONFIG_DIR") {
        let trimmed = claude_config_dir.trim();
        if !trimmed.is_empty() {
            let p = PathBuf::from(trimmed).join("projects");
            if !dirs.contains(&p) {
                dirs.push(p);
            }
        }
    }

    // %APPDATA%/Claude/projects  (Windows)
    if let Some(config) = dirs::config_dir() {
        let p = config.join("Claude").join("projects");
        if !dirs.contains(&p) {
            dirs.push(p);
        }
    }

    for home in candidate_home_dirs() {
        let p = home.join(".claude").join("projects");
        if !dirs.contains(&p) {
            dirs.push(p);
        }
    }

    dirs
}

/// Return the first existing Claude Code projects dir.
pub fn claude_projects_dir() -> Option<PathBuf> {
    claude_projects_dirs().into_iter().find(|p| p.exists())
}

// ── Clipboard ──

pub fn copy_to_clipboard(text: &str) -> Result<(), String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| format!("Clipboard error: {e}"))?;
    clipboard
        .set_text(text)
        .map_err(|e| format!("Clipboard set error: {e}"))
}

// ── File I/O helpers ──

/// Read first `head_n` and last `tail_n` lines from a file.
pub fn read_head_tail_lines(
    path: &Path,
    head_n: usize,
    tail_n: usize,
) -> Result<(Vec<String>, Vec<String>), String> {
    let file =
        std::fs::File::open(path).map_err(|e| format!("Cannot open {}: {e}", path.display()))?;
    let file_len = file.metadata().map(|m| m.len()).unwrap_or(0);

    if file_len < 16_384 {
        let reader = BufReader::new(file);
        let all: Vec<String> = reader.lines().filter_map(|l| l.ok()).collect();
        let head: Vec<_> = all.iter().take(head_n).cloned().collect();
        let skip = all.len().saturating_sub(tail_n);
        let tail: Vec<_> = all.into_iter().skip(skip).collect();
        return Ok((head, tail));
    }

    let reader = BufReader::new(file);
    let head: Vec<String> = reader
        .lines()
        .take(head_n)
        .filter_map(|l| l.ok())
        .collect();

    let seek_pos = file_len.saturating_sub(16_384);
    let mut file2 =
        std::fs::File::open(path).map_err(|e| format!("Cannot reopen {}: {e}", path.display()))?;
    file2.seek(SeekFrom::Start(seek_pos)).map_err(|e| {
        format!("Cannot seek {}: {e}", path.display())
    })?;
    let tail_reader = BufReader::new(file2);
    let all_tail: Vec<String> = tail_reader.lines().filter_map(|l| l.ok()).collect();
    let skip_first = if seek_pos > 0 { 1 } else { 0 };
    let usable: Vec<_> = all_tail.into_iter().skip(skip_first).collect();
    let skip = usable.len().saturating_sub(tail_n);
    let tail: Vec<_> = usable.into_iter().skip(skip).collect();

    Ok((head, tail))
}

// ── Timestamp parsing ──

pub fn parse_timestamp_to_ms(value: &Value) -> Option<i64> {
    if let Some(n) = value.as_i64() {
        return Some(if n > 1_000_000_000_000 { n } else { n * 1000 });
    }
    if let Some(n) = value.as_f64() {
        let n = n as i64;
        return Some(if n > 1_000_000_000_000 { n } else { n * 1000 });
    }
    let raw = value.as_str()?;
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|dt: DateTime<FixedOffset>| dt.timestamp_millis())
}

pub fn format_timestamp(ms: i64) -> String {
    if let Some(dt) = chrono::DateTime::from_timestamp_millis(ms) {
        dt.format("%Y-%m-%d %H:%M:%S").to_string()
    } else {
        ms.to_string()
    }
}

// ── Text extraction ──

pub fn extract_text(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(items) => items
            .iter()
            .filter_map(extract_text_from_item)
            .filter(|t| !t.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Object(map) => map
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        _ => String::new(),
    }
}

pub fn extract_text_from_item(item: &Value) -> Option<String> {
    let item_type = item.get("type").and_then(Value::as_str).unwrap_or("");

    if item_type == "tool_use" {
        let name = item
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        return Some(format!("[Tool: {name}]"));
    }

    if item_type == "tool_result" {
        if let Some(content) = item.get("content") {
            let text = extract_text(content);
            if !text.is_empty() {
                return Some(text);
            }
        }
        return None;
    }

    if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
        return Some(text.to_string());
    }

    if let Some(text) = item.get("input_text").and_then(|v| v.as_str()) {
        return Some(text.to_string());
    }

    if let Some(text) = item.get("output_text").and_then(|v| v.as_str()) {
        return Some(text.to_string());
    }

    if let Some(content) = item.get("content") {
        let text = extract_text(content);
        if !text.is_empty() {
            return Some(text);
        }
    }

    None
}

// ── String helpers ──

pub fn truncate_summary(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let mut result: String = trimmed.chars().take(max_chars).collect();
    result.push_str("...");
    result
}

pub fn path_basename(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let normalized = trimmed.trim_end_matches(['/', '\\']);
    let last = normalized
        .split(['/', '\\'])
        .next_back()
        .filter(|s| !s.is_empty())?;
    Some(last.to_string())
}

// ── Terminal launch (Windows) ──

pub fn launch_terminal(command: &str, cwd: Option<&str>) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        launch_windows_terminal(command, cwd)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (command, cwd);
        Err("Terminal launch is only supported on Windows in this build".to_string())
    }
}

#[cfg(target_os = "windows")]
fn launch_windows_terminal(command: &str, cwd: Option<&str>) -> Result<(), String> {
    use std::process::Command;

    let full_cmd = match cwd {
        Some(dir) if !dir.trim().is_empty() => format!("cd /d \"{}\" && {}", dir, command),
        _ => command.to_string(),
    };

    // Try Windows Terminal first, fall back to cmd
    let wt_result = Command::new("wt")
        .arg("-d")
        .arg(cwd.unwrap_or("."))
        .arg("cmd")
        .arg("/k")
        .arg(&full_cmd)
        .spawn();

    match wt_result {
        Ok(_) => return Ok(()),
        Err(_) => {}
    }

    // Fallback: plain cmd
    Command::new("cmd")
        .arg("/c")
        .arg("start")
        .arg("cmd")
        .arg("/k")
        .arg(&full_cmd)
        .spawn()
        .map_err(|e| format!("Failed to launch terminal: {e}"))?;

    Ok(())
}
