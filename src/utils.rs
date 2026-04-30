use std::collections::HashSet;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use chrono::{DateTime, FixedOffset};
use serde_json::Value;

pub(crate) const MAX_SCAN_DEPTH: usize = 64;
pub(crate) const MAX_SCAN_FILES: usize = 50_000;

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

    // macOS: ~/Library/Application Support/Codex/sessions
    // Windows: %LOCALAPPDATA%/Codex/sessions
    if let Some(data) = dirs::data_local_dir() {
        let p = data.join("Codex").join("sessions");
        if !dirs.contains(&p) {
            dirs.push(p);
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

pub fn collect_files_safely(root: &Path, extension: &str) -> Vec<PathBuf> {
    collect_files_safely_with_limits(root, extension, MAX_SCAN_DEPTH, MAX_SCAN_FILES)
}

fn collect_files_safely_with_limits(
    root: &Path,
    extension: &str,
    max_depth: usize,
    max_files: usize,
) -> Vec<PathBuf> {
    let extension = extension.trim_start_matches('.');
    if extension.is_empty() || max_files == 0 {
        return Vec::new();
    }

    let root_meta = match std::fs::symlink_metadata(root) {
        Ok(meta) => meta,
        Err(_) => return Vec::new(),
    };
    if is_link_or_reparse_point(&root_meta) || !root_meta.is_dir() {
        return Vec::new();
    }

    let root = match root.canonicalize() {
        Ok(path) => path,
        Err(_) => return Vec::new(),
    };

    let mut files = Vec::new();
    let mut visited = HashSet::new();
    visited.insert(root.clone());

    let mut stack = vec![(root.clone(), 0usize)];
    while let Some((current, depth)) = stack.pop() {
        let entries = match std::fs::read_dir(&current) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let metadata = match std::fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(_) => continue,
            };

            if is_link_or_reparse_point(&metadata) {
                continue;
            }

            if metadata.is_dir() {
                if depth >= max_depth {
                    continue;
                }
                let resolved = match path.canonicalize() {
                    Ok(path) => path,
                    Err(_) => continue,
                };
                if resolved.starts_with(&root) && visited.insert(resolved.clone()) {
                    stack.push((resolved, depth + 1));
                }
                continue;
            }

            if !metadata.is_file() || !path_has_extension(&path, extension) {
                continue;
            }

            let resolved = match path.canonicalize() {
                Ok(path) => path,
                Err(_) => continue,
            };
            if resolved.starts_with(&root) {
                files.push(resolved);
                if files.len() >= max_files {
                    files.sort();
                    return files;
                }
            }
        }
    }

    files.sort();
    files
}

fn path_has_extension(path: &Path, extension: &str) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .map(|value| value.eq_ignore_ascii_case(extension))
        .unwrap_or(false)
}

pub(crate) fn is_link_or_reparse_point(metadata: &std::fs::Metadata) -> bool {
    metadata.file_type().is_symlink() || has_windows_reparse_point(metadata)
}

#[cfg(target_os = "windows")]
fn has_windows_reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(target_os = "windows"))]
fn has_windows_reparse_point(_metadata: &std::fs::Metadata) -> bool {
    false
}

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

pub fn resume_command(provider_id: &str, session_id: &str) -> Option<String> {
    if !is_safe_session_id(session_id) {
        return None;
    }

    match provider_id {
        "opencode" => Some(format!("opencode --session {session_id}")),
        "claude" => Some(format!("claude --resume {session_id}")),
        "codex" => Some(format!("codex resume {session_id}")),
        _ => None,
    }
}

fn is_safe_session_id(session_id: &str) -> bool {
    !session_id.is_empty()
        && session_id.len() <= 256
        && session_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
}

fn resume_command_parts<'a>(
    provider_id: &str,
    session_id: &'a str,
) -> Result<(&'static str, Vec<&'a str>), String> {
    if !is_safe_session_id(session_id) {
        return Err("Unsafe session ID; refusing to launch".to_string());
    }

    match provider_id {
        "opencode" => Ok(("opencode", vec!["--session", session_id])),
        "claude" => Ok(("claude", vec!["--resume", session_id])),
        "codex" => Ok(("codex", vec!["resume", session_id])),
        _ => Err(format!("Unsupported provider: {provider_id}")),
    }
}

pub fn launch_resume_terminal(
    provider_id: &str,
    session_id: &str,
    cwd: Option<&str>,
) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        launch_windows_resume_terminal(provider_id, session_id, cwd)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (provider_id, session_id, cwd);
        Err("Terminal launch is only supported on Windows in this build".to_string())
    }
}

#[cfg(target_os = "windows")]
fn launch_windows_resume_terminal(
    provider_id: &str,
    session_id: &str,
    cwd: Option<&str>,
) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    let (program, args) = resume_command_parts(provider_id, session_id)?;
    let cwd = cwd.filter(|value| !value.trim().is_empty()).unwrap_or(".");

    // Try Windows Terminal first. Program and arguments are passed separately,
    // so a malicious session id cannot be interpreted as shell syntax.
    let wt_result = Command::new("wt")
        .arg("-d")
        .arg(cwd)
        .arg(program)
        .args(&args)
        .spawn();

    match wt_result {
        Ok(_) => return Ok(()),
        Err(_) => {}
    }

    const CREATE_NEW_CONSOLE: u32 = 0x00000010;

    let mut fallback = Command::new(program);
    fallback
        .args(&args)
        .current_dir(cwd)
        .creation_flags(CREATE_NEW_CONSOLE);

    fallback
        .spawn()
        .map_err(|e| format!("Failed to launch terminal: {e}"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        collect_files_safely, collect_files_safely_with_limits, resume_command,
        resume_command_parts, MAX_SCAN_DEPTH,
    };
    use std::path::{Path, PathBuf};

    #[test]
    fn resume_command_allows_expected_session_ids() {
        assert_eq!(
            resume_command("opencode", "ses_abc-123.def"),
            Some("opencode --session ses_abc-123.def".to_string())
        );
        assert_eq!(
            resume_command("claude", "550e8400-e29b-41d4-a716-446655440000"),
            Some("claude --resume 550e8400-e29b-41d4-a716-446655440000".to_string())
        );
        assert_eq!(
            resume_command("codex", "550e8400-e29b-41d4-a716-446655440000"),
            Some("codex resume 550e8400-e29b-41d4-a716-446655440000".to_string())
        );
    }

    #[test]
    fn resume_command_rejects_shell_metacharacters() {
        assert_eq!(resume_command("opencode", "ses_abc&calc"), None);
        assert_eq!(resume_command("claude", "abc|calc"), None);
        assert_eq!(resume_command("codex", "abc\" & calc"), None);
    }

    #[test]
    fn launch_parts_reject_shell_metacharacters() {
        let err = resume_command_parts("opencode", "ses_abc&&calc").unwrap_err();
        assert!(err.contains("Unsafe session ID"));
    }

    #[test]
    fn collect_files_safely_collects_nested_extension_matches() {
        let root = test_root("collects");
        let _ = std::fs::remove_dir_all(&root);

        let wanted = root.join("a").join("b").join("one.jsonl");
        write_file(&wanted, "{}");
        write_file(&root.join("a").join("b").join("two.txt"), "ignored");

        let files = collect_files_safely(&root, "jsonl");

        assert_eq!(files.len(), 1);
        assert_eq!(files[0], wanted.canonicalize().unwrap());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn collect_files_safely_respects_depth_limit() {
        let root = test_root("depth");
        let _ = std::fs::remove_dir_all(&root);

        let mut current = root.clone();
        for _ in 0..=MAX_SCAN_DEPTH {
            current = current.join("d");
        }
        let too_deep = current.join("too_deep.json");
        write_file(&too_deep, "{}");

        let files = collect_files_safely(&root, "json");

        assert!(!files
            .iter()
            .any(|path| path.file_name().and_then(|n| n.to_str()) == Some("too_deep.json")));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn collect_files_safely_respects_file_limit() {
        let root = test_root("limit");
        let _ = std::fs::remove_dir_all(&root);

        for i in 0..5 {
            write_file(&root.join(format!("{i}.json")), "{}");
        }

        let files = collect_files_safely_with_limits(&root, "json", MAX_SCAN_DEPTH, 3);

        assert_eq!(files.len(), 3);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn collect_files_safely_skips_symlink_dirs() {
        let root = test_root("symlink-root");
        let outside = test_root("symlink-outside");
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&outside);

        write_file(&root.join("inside.json"), "{}");
        write_file(&outside.join("outside.json"), "{}");

        if create_dir_link(&outside, &root.join("linked")).is_err() {
            let _ = std::fs::remove_dir_all(&root);
            let _ = std::fs::remove_dir_all(&outside);
            return;
        }

        let files = collect_files_safely(&root, "json");

        assert_eq!(files.len(), 1);
        assert_eq!(
            files[0].file_name().and_then(|n| n.to_str()),
            Some("inside.json")
        );

        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&outside);
    }

    fn test_root(name: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::current_dir()
            .unwrap()
            .join("target")
            .join("safe-scan-tests")
            .join(format!("{name}-{}-{unique}", std::process::id()))
    }

    fn write_file(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, contents).unwrap();
    }

    #[cfg(target_family = "unix")]
    fn create_dir_link(target: &Path, link: &Path) -> std::io::Result<()> {
        std::os::unix::fs::symlink(target, link)
    }

    #[cfg(target_os = "windows")]
    fn create_dir_link(target: &Path, link: &Path) -> std::io::Result<()> {
        std::os::windows::fs::symlink_dir(target, link)
    }

    #[cfg(not(any(target_family = "unix", target_os = "windows")))]
    fn create_dir_link(_target: &Path, _link: &Path) -> std::io::Result<()> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "directory links are not supported on this platform",
        ))
    }
}
