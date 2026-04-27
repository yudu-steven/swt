pub mod claude;
pub mod codex;
pub mod opencode;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct SessionMeta {
    pub provider_id: String,
    pub session_id: String,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub project_dir: Option<String>,
    pub created_at: Option<i64>,
    pub last_active_at: Option<i64>,
    pub source_path: Option<String>,
    pub resume_command: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionMessage {
    pub role: String,
    pub content: String,
    pub ts: Option<i64>,
}

/// Scan all providers and return merged, sorted sessions.
pub fn scan_all() -> Vec<SessionMeta> {
    let (r1, r2, r3) = std::thread::scope(|s| {
        let h1 = s.spawn(opencode::scan_sessions);
        let h2 = s.spawn(claude::scan_sessions);
        let h3 = s.spawn(codex::scan_sessions);
        (
            h1.join().unwrap_or_default(),
            h2.join().unwrap_or_default(),
            h3.join().unwrap_or_default(),
        )
    });

    let mut sessions = Vec::new();
    sessions.extend(r1);
    sessions.extend(r2);
    sessions.extend(r3);

    sessions.sort_by(|a, b| {
        let a_ts = a.last_active_at.or(a.created_at).unwrap_or(0);
        let b_ts = b.last_active_at.or(b.created_at).unwrap_or(0);
        b_ts.cmp(&a_ts)
    });

    sessions
}

/// Load messages for a given provider and source path.
pub fn load_messages(provider_id: &str, source_path: &str) -> Result<Vec<SessionMessage>, String> {
    if provider_id == "opencode" && source_path.starts_with("sqlite:") {
        return opencode::load_messages_sqlite(source_path);
    }

    let path = std::path::Path::new(source_path);
    match provider_id {
        "opencode" => opencode::load_messages(path),
        "claude" => claude::load_messages(path),
        "codex" => codex::load_messages(path),
        _ => Err(format!("Unsupported provider: {provider_id}")),
    }
}

/// Delete a session.
pub fn delete_session(
    provider_id: &str,
    session_id: &str,
    source_path: &str,
) -> Result<bool, String> {
    if provider_id == "opencode" && source_path.starts_with("sqlite:") {
        return opencode::delete_session_sqlite(session_id, source_path);
    }

    let path = std::path::Path::new(source_path);
    let root = match provider_id {
        "opencode" => crate::utils::opencode_base_dir().unwrap_or_default(),
        "claude" => crate::utils::claude_projects_dir().unwrap_or_default(),
        "codex" => crate::utils::codex_sessions_dir().unwrap_or_default(),
        _ => return Err(format!("Unsupported provider: {provider_id}")),
    };

    let validated_root = canonicalize_existing_path(&root, "session root")?;
    let validated_source = canonicalize_existing_path(path, "session source")?;

    if !validated_source.starts_with(&validated_root) {
        return Err(format!("Source path is outside provider root"));
    }

    match provider_id {
        "opencode" => opencode::delete_session(&validated_root, &validated_source, session_id),
        "claude" => claude::delete_session(&validated_root, &validated_source, session_id),
        "codex" => codex::delete_session(&validated_root, &validated_source, session_id),
        _ => Err(format!("Unsupported provider: {provider_id}")),
    }
}

fn canonicalize_existing_path(path: &std::path::Path, label: &str) -> Result<std::path::PathBuf, String> {
    if !path.exists() {
        return Err(format!("{label} not found: {}", path.display()));
    }
    path.canonicalize()
        .map_err(|e| format!("Failed to resolve {label}: {e}"))
}
