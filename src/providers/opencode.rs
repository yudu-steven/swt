use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde_json::Value;

use crate::utils::{
    parse_timestamp_to_ms, path_basename, truncate_summary,
};

use super::{SessionMessage, SessionMeta};

const PROVIDER_ID: &str = "opencode";

// ── Path helpers ──

fn opencode_db_paths() -> Vec<PathBuf> {
    crate::utils::opencode_base_dirs()
        .into_iter()
        .map(|d| d.join("opencode.db"))
        .filter(|p| p.exists())
        .collect()
}

fn opencode_storage_dirs() -> Vec<PathBuf> {
    crate::utils::opencode_base_dirs()
        .into_iter()
        .map(|d| d.join("storage"))
        .filter(|p| p.exists())
        .collect()
}

// ── Scan ──

pub fn scan_sessions() -> Vec<SessionMeta> {
    let json_sessions = scan_json();
    let sqlite_sessions = scan_sqlite();

    if sqlite_sessions.is_empty() {
        return json_sessions;
    }
    if json_sessions.is_empty() {
        return sqlite_sessions;
    }

    let sqlite_ids: std::collections::HashSet<String> =
        sqlite_sessions.iter().map(|s| s.session_id.clone()).collect();

    let mut merged = sqlite_sessions;
    for s in json_sessions {
        if !sqlite_ids.contains(&s.session_id) {
            merged.push(s);
        }
    }
    merged
}

fn scan_json() -> Vec<SessionMeta> {
    let mut sessions = Vec::new();
    for storage in opencode_storage_dirs() {
        let session_dir = storage.join("session");
        if !session_dir.exists() {
            continue;
        }
        let mut json_files = Vec::new();
        collect_json_files(&session_dir, &mut json_files);
        for path in json_files {
            if let Some(meta) = parse_session_json(&storage, &path) {
                sessions.push(meta);
            }
        }
    }
    sessions
}

fn scan_sqlite() -> Vec<SessionMeta> {
    let mut sessions = Vec::new();
    for db_path in opencode_db_paths() {
        let conn = match Connection::open_with_flags(
            &db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        ) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let mut stmt = match conn.prepare(
            "SELECT id, title, directory, time_created, time_updated FROM session ORDER BY time_updated DESC",
        ) {
            Ok(s) => s,
            Err(_) => continue,
        };

        let db_display = db_path.display().to_string();

        let iter = match stmt.query_map([], |row| {
            let session_id: String = row.get(0)?;
            let title: String = row.get(1)?;
            let directory: String = row.get(2)?;
            let created: i64 = row.get(3)?;
            let updated: i64 = row.get(4)?;
            Ok((session_id, title, directory, created, updated))
        }) {
            Ok(rows) => rows,
            Err(_) => continue,
        };

        for row in iter.flatten() {
            let (session_id, title, directory, created, updated) = row;
            let display_title = if title.is_empty() {
                path_basename(&directory)
            } else {
                Some(title)
            };
            sessions.push(SessionMeta {
                provider_id: PROVIDER_ID.to_string(),
                session_id: session_id.clone(),
                title: None,
                summary: display_title,
                project_dir: if directory.is_empty() {
                    None
                } else {
                    Some(directory)
                },
                created_at: Some(created),
                last_active_at: Some(updated),
                source_path: Some(format!("sqlite:{db_display}:{session_id}")),
                resume_command: Some(format!("opencode --session {session_id}")),
            });
        }
    }
    sessions
}

fn parse_session_json(storage: &Path, path: &Path) -> Option<SessionMeta> {
    let data = std::fs::read_to_string(path).ok()?;
    let value: Value = serde_json::from_str(&data).ok()?;

    let session_id = value.get("id").and_then(Value::as_str)?.to_string();
    let title = value
        .get("title")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let directory = value
        .get("directory")
        .and_then(Value::as_str)
        .map(|s| s.to_string());

    let created_at = value
        .get("time")
        .and_then(|t| t.get("created"))
        .and_then(parse_timestamp_to_ms);
    let updated_at = value
        .get("time")
        .and_then(|t| t.get("updated"))
        .and_then(parse_timestamp_to_ms);

    let has_title = title.is_some();
    let display_title = title.or_else(|| {
        directory
            .as_deref()
            .and_then(path_basename)
            .map(|s| s.to_string())
    });

    let msg_dir = storage.join("message").join(&session_id);
    let source_path = msg_dir.to_string_lossy().to_string();

    let summary = if has_title {
        display_title.clone()
    } else {
        get_first_user_summary(storage, &session_id)
    };

    Some(SessionMeta {
        provider_id: PROVIDER_ID.to_string(),
        session_id: session_id.clone(),
        title: None,
        summary,
        project_dir: directory,
        created_at,
        last_active_at: updated_at.or(created_at),
        source_path: Some(source_path),
        resume_command: Some(format!("opencode --session {session_id}")),
    })
}

fn get_first_user_summary(storage: &Path, session_id: &str) -> Option<String> {
    let msg_dir = storage.join("message").join(session_id);
    if !msg_dir.is_dir() {
        return None;
    }

    let mut msg_files = Vec::new();
    collect_json_files(&msg_dir, &mut msg_files);

    let mut user_msgs: Vec<(i64, String)> = Vec::new();
    for msg_path in &msg_files {
        let data = std::fs::read_to_string(msg_path).ok()?;
        let value: Value = serde_json::from_str(&data).ok()?;

        if value.get("role").and_then(Value::as_str) != Some("user") {
            continue;
        }
        let msg_id = value.get("id").and_then(Value::as_str)?.to_string();
        let ts = value
            .get("time")
            .and_then(|t| t.get("created"))
            .and_then(parse_timestamp_to_ms)
            .unwrap_or(0);
        user_msgs.push((ts, msg_id));
    }

    user_msgs.sort_by_key(|(ts, _)| *ts);
    let (_, first_id) = user_msgs.first()?;
    let part_dir = storage.join("part").join(first_id);
    let text = collect_parts_text(&part_dir);
    if text.trim().is_empty() {
        return None;
    }
    Some(truncate_summary(&text, 160))
}

// ── Load messages ──

pub fn load_messages(path: &Path) -> Result<Vec<SessionMessage>, String> {
    if !path.is_dir() {
        return Err(format!("Message directory not found: {}", path.display()));
    }

    let storage = path
        .parent()
        .and_then(|p| p.parent())
        .ok_or_else(|| "Cannot determine storage root".to_string())?;

    let mut msg_files = Vec::new();
    collect_json_files(path, &mut msg_files);

    let mut entries: Vec<(i64, String, String)> = Vec::new();

    for msg_path in &msg_files {
        let data = std::fs::read_to_string(msg_path)
            .map_err(|_| ())
            .unwrap_or_default();
        let value: Value = serde_json::from_str(&data).unwrap_or_default();

        let msg_id = match value.get("id").and_then(Value::as_str) {
            Some(id) => id.to_string(),
            None => continue,
        };
        let role = value
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        let created_ts = value
            .get("time")
            .and_then(|t| t.get("created"))
            .and_then(parse_timestamp_to_ms)
            .unwrap_or(0);

        let part_dir = storage.join("part").join(&msg_id);
        let text = collect_parts_text(&part_dir);
        if text.trim().is_empty() {
            continue;
        }

        entries.push((created_ts, role, text));
    }

    entries.sort_by_key(|(ts, _, _)| *ts);

    let messages = entries
        .into_iter()
        .map(|(ts, role, content)| SessionMessage {
            role,
            content,
            ts: if ts > 0 { Some(ts) } else { None },
        })
        .collect();

    Ok(messages)
}

pub fn load_messages_sqlite(source: &str) -> Result<Vec<SessionMessage>, String> {
    let (db_path, session_id) = parse_sqlite_source(source)
        .ok_or_else(|| format!("Invalid SQLite source reference: {source}"))?;

    let conn = Connection::open_with_flags(
        &db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("Failed to open database: {e}"))?;

    // Load messages
    let mut msg_stmt = conn
        .prepare(
            "SELECT id, time_created, data FROM message WHERE session_id = ?1 ORDER BY time_created ASC",
        )
        .map_err(|e| format!("Failed to prepare query: {e}"))?;

    let msg_rows = msg_stmt
        .query_map([session_id.as_str()], |row| {
            let id: String = row.get(0)?;
            let ts: i64 = row.get(1)?;
            let data: String = row.get(2)?;
            Ok((id, ts, data))
        })
        .map_err(|e| format!("Failed to query messages: {e}"))?;

    // Load parts
    let mut part_stmt = conn
        .prepare(
            "SELECT message_id, data FROM part WHERE session_id = ?1 ORDER BY time_created ASC",
        )
        .map_err(|e| format!("Failed to prepare part query: {e}"))?;

    let part_rows = part_stmt
        .query_map([session_id.as_str()], |row| {
            let message_id: String = row.get(0)?;
            let data: String = row.get(1)?;
            Ok((message_id, data))
        })
        .map_err(|e| format!("Failed to query parts: {e}"))?;

    let mut parts_map: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for part in part_rows.flatten() {
        let (message_id, data) = part;
        parts_map.entry(message_id).or_default().push(data);
    }

    let mut messages = Vec::new();
    for row in msg_rows.flatten() {
        let (msg_id, ts, data) = row;
        let msg_value: Value = match serde_json::from_str(&data) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let role = msg_value
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();

        let content = if let Some(parts) = parts_map.get(&msg_id) {
            let texts: Vec<String> = parts
                .iter()
                .filter_map(|part_data| {
                    let part_value: Value = serde_json::from_str(part_data).ok()?;
                    extract_part_text(&part_value)
                })
                .collect();
            texts.join("\n")
        } else {
            continue;
        };

        if content.trim().is_empty() {
            continue;
        }

        messages.push(SessionMessage {
            role,
            content,
            ts: Some(ts),
        });
    }

    Ok(messages)
}

fn parse_sqlite_source(source: &str) -> Option<(PathBuf, String)> {
    let rest = source.strip_prefix("sqlite:")?;
    let sep = rest.rfind(":ses_")?;
    let db_path = PathBuf::from(&rest[..sep]);
    let session_id = rest[sep + 1..].to_string();
    Some((db_path, session_id))
}

// ── Delete ──

pub fn delete_session(storage: &Path, path: &Path, session_id: &str) -> Result<bool, String> {
    if path.file_name().and_then(|n| n.to_str()) != Some(session_id) {
        return Err(format!("Session path does not match ID"));
    }

    let mut msg_files = Vec::new();
    collect_json_files(path, &mut msg_files);

    let mut msg_ids = Vec::new();
    for msg_path in &msg_files {
        if let Ok(data) = std::fs::read_to_string(msg_path) {
            if let Ok(value) = serde_json::from_str::<Value>(&data) {
                if let Some(id) = value.get("id").and_then(Value::as_str) {
                    msg_ids.push(id.to_string());
                }
            }
        }
    }

    for msg_id in &msg_ids {
        let part_dir = storage.join("part").join(msg_id);
        let _ = std::fs::remove_dir_all(&part_dir);
    }

    let session_diff = storage
        .join("session_diff")
        .join(format!("{session_id}.json"));
    let _ = std::fs::remove_file(&session_diff);

    let _ = std::fs::remove_dir_all(path);

    if let Some(session_file) = find_session_file(storage, session_id) {
        let _ = std::fs::remove_file(&session_file);
    }

    Ok(true)
}

pub fn delete_session_sqlite(session_id: &str, source: &str) -> Result<bool, String> {
    let (db_path, ref_id) = parse_sqlite_source(source)
        .ok_or_else(|| format!("Invalid SQLite source: {source}"))?;
    let db_path = db_path
        .canonicalize()
        .map_err(|e| format!("Failed to resolve db path: {e}"))?;

    // Check against all candidate db paths
    let all_candidates: Vec<PathBuf> = opencode_db_paths()
        .into_iter()
        .filter_map(|p| p.canonicalize().ok())
        .collect();

    if ref_id != session_id {
        return Err("Session ID mismatch".to_string());
    }
    if !all_candidates.contains(&db_path) {
        return Err("Database path not found in known OpenCode locations".to_string());
    }

    let conn =
        Connection::open(&db_path).map_err(|e| format!("Failed to open database: {e}"))?;
    let tx = conn.unchecked_transaction().map_err(|e| format!("{e}"))?;

    tx.execute("DELETE FROM part WHERE session_id = ?1", [session_id])
        .map_err(|e| format!("{e}"))?;
    tx.execute("DELETE FROM message WHERE session_id = ?1", [session_id])
        .map_err(|e| format!("{e}"))?;
    let deleted = tx
        .execute("DELETE FROM session WHERE id = ?1", [session_id])
        .map_err(|e| format!("{e}"))?;

    tx.commit().map_err(|e| format!("{e}"))?;
    Ok(deleted > 0)
}

// ── Helpers ──

fn extract_part_text(part_value: &Value) -> Option<String> {
    match part_value.get("type").and_then(Value::as_str) {
        Some("text") => part_value
            .get("text")
            .and_then(Value::as_str)
            .filter(|t| !t.trim().is_empty())
            .map(|t| t.to_string()),
        Some("tool") => {
            let tool = part_value
                .get("tool")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            Some(format!("[Tool: {tool}]"))
        }
        _ => None,
    }
}

fn collect_parts_text(part_dir: &Path) -> String {
    if !part_dir.is_dir() {
        return String::new();
    }

    let mut parts = Vec::new();
    collect_json_files(part_dir, &mut parts);

    let mut texts = Vec::new();
    for part_path in &parts {
        if let Ok(data) = std::fs::read_to_string(part_path) {
            if let Ok(value) = serde_json::from_str::<Value>(&data) {
                if let Some(text) = extract_part_text(&value) {
                    texts.push(text);
                }
            }
        }
    }
    texts.join("\n")
}

fn collect_json_files(root: &Path, files: &mut Vec<PathBuf>) {
    if !root.exists() {
        return;
    }
    let entries = match std::fs::read_dir(root) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_json_files(&path, files);
        } else if path.extension().and_then(|e| e.to_str()) == Some("json") {
            files.push(path);
        }
    }
}

fn find_session_file(storage: &Path, session_id: &str) -> Option<PathBuf> {
    let session_root = storage.join("session");
    let mut files = Vec::new();
    collect_json_files(&session_root, &mut files);
    let expected = format!("{session_id}.json");
    files
        .into_iter()
        .find(|p| p.file_name().and_then(|n| n.to_str()) == Some(&expected))
}
