mod providers;
mod utils;

use clap::{Parser, Subcommand};
use colored::Colorize;
use providers::{delete_session, load_messages, scan_all, SessionMeta};
use utils::{copy_to_clipboard, format_timestamp, launch_terminal};

const BANNER: &str = r#"
  ╔══════════════════════════════════════════════╗
  ║                ⚡  swt  ⚡                    ║
  ║   AI Coding Session Manager                  ║
  ╚══════════════════════════════════════════════╝
"#;

/// swt — Browse, resume & delete AI coding sessions
#[derive(Parser)]
#[command(name = "swt", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// List all sessions
    #[command(visible_alias = "ls")]
    List {
        /// Filter by provider (opencode, claude, codex)
        provider: Option<String>,
        /// Search keyword
        #[arg(short, long)]
        search: Option<String>,
        /// Limit results
        #[arg(short, long)]
        limit: Option<usize>,
    },
    /// Show conversation messages
    #[command(visible_alias = "cat", visible_alias = "view")]
    Show {
        /// Session ID (prefix ok)
        id: String,
        /// Provider hint
        #[arg(short, long)]
        provider: Option<String>,
        /// Search within messages
        #[arg(short, long)]
        search: Option<String>,
    },
    /// Copy resume command to clipboard
    #[command(visible_alias = "res")]
    Resume {
        /// Session ID (prefix ok)
        id: String,
        /// Provider hint
        #[arg(short, long)]
        provider: Option<String>,
        /// Only copy, don't launch terminal
        #[arg(long)]
        copy: bool,
        /// Launch in terminal window
        #[arg(long)]
        launch: bool,
    },
    /// Delete a local session
    #[command(visible_alias = "rm")]
    Delete {
        /// Session ID
        id: String,
        /// Provider hint
        #[arg(short, long)]
        provider: Option<String>,
        /// Skip confirmation
        #[arg(long)]
        force: bool,
    },
    /// Show data source paths and counts
    Info,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::List {
            provider,
            search,
            limit,
        }) => cmd_list(provider.as_deref(), search.as_deref(), limit),
        Some(Commands::Show {
            id,
            provider,
            search,
        }) => cmd_show(&id, provider.as_deref(), search.as_deref()),
        Some(Commands::Resume {
            id,
            provider,
            copy,
            launch,
        }) => cmd_resume(&id, provider.as_deref(), copy, launch),
        Some(Commands::Delete {
            id,
            provider,
            force,
        }) => cmd_delete(&id, provider.as_deref(), force),
        Some(Commands::Info) => cmd_info(),
        None => cmd_interactive(),
    }
}

// ═══════════════════════════════════════════════════════════
//  INTERACTIVE MODE
// ═══════════════════════════════════════════════════════════

fn cmd_interactive() {
    use dialoguer::{theme::ColorfulTheme, Select};
    use console::Term;

    let term = Term::stdout();
    let _ = term.clear_screen();

    println!("{}", BANNER.bright_cyan());

    loop {
        let sessions = scan_all();
        if sessions.is_empty() {
            println!(
                "\n  {}",
                "No sessions found. Make sure OpenCode / Claude Code / Codex has been used on this machine."
                    .yellow()
            );
            println!(
                "  {}",
                "Run 'switch info' to check data paths.".dimmed()
            );
            return;
        }

        // ── Build menu items ──
        let mut items: Vec<String> = Vec::new();
        for s in &sessions {
            let provider_icon = provider_icon(&s.provider_id);
            let title = format_session_title(s);
            let project = s
                .project_dir
                .as_deref()
                .and_then(utils::path_basename)
                .unwrap_or_default();
            let time = s.last_active_at.map(|t| {
                let dt = chrono::DateTime::from_timestamp_millis(t)
                    .map(|d| d.format("%m-%d %H:%M").to_string())
                    .unwrap_or_default();
                dt
            }).unwrap_or_default();

            items.push(format!(
                "{:>12}  {:<45}  {:>5}  {}",
                provider_icon,
                trunc(&title, 42),
                project,
                time
            ));
        }

        // ── Selection ──
        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt(format!(
                "{} (↑↓ move  enter view  r resume  d delete  / search  q quit)",
                format!("{} sessions", sessions.len()).bold()
            ))
            .items(&items)
            .default(0)
            .interact_opt()
            .unwrap_or(None);

        match selection {
            Some(idx) => {
                let _ = term.clear_screen();
                let session = &sessions[idx];
                session_detail_view(session);
            }
            None => break,
        }
    }
}

fn session_detail_view(session: &SessionMeta) {
    use dialoguer::{theme::ColorfulTheme, Select, Confirm};
    use console::Term;

    let term = Term::stdout();

    loop {
        let _ = term.clear_screen();

        // ── Header card ──
        let width = 64;
        let top = format!("╔{}╗", "═".repeat(width - 2));
        let bot = format!("╚{}╝", "═".repeat(width - 2));

        println!("{}", BANNER.bright_cyan());
        println!("{}", top.dimmed());

        let icon = provider_icon(&session.provider_id);
        let provider = provider_label(&session.provider_id);
        println!(
            "║ {}  {:<w$}║",
            icon,
            format!("{}  —  {}", provider.bold(), session.session_id.dimmed()),
            w = width - 6
        );

        if let Some(ref title) = session.title {
            println!("║  {:<w$}║", format!("📝  {}", title), w = width - 4);
        }
        if let Some(ref dir) = session.project_dir {
            println!("║  {:<w$}║", format!("📁  {}", dir), w = width - 4);
        }
        if let Some(ts) = session.created_at {
            println!("║  {:<w$}║", format!("🕐  Created: {}", format_timestamp(ts)), w = width - 4);
        }
        if let Some(ts) = session.last_active_at {
            println!("║  {:<w$}║", format!("🕑  Updated: {}", format_timestamp(ts)), w = width - 4);
        }
        if let Some(ref cmd) = session.resume_command {
            println!("║  {:<w$}║", format!("▶  Resume: {}", cmd.bright_green()), w = width - 4);
        }
        println!("{}", bot.dimmed());
        println!();

        // ── Actions ──
        let actions = vec![
            "📖  View conversation",
            "📋  Copy resume command",
            "🚀  Launch in terminal",
            "🗑   Delete session",
            "↩   Back to list",
        ];

        let choice = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("What do you want to do?")
            .items(&actions)
            .default(0)
            .interact()
            .unwrap_or(4);

        match choice {
            0 => {
                // View messages
                let _ = term.clear_screen();
                view_messages(session);
            }
            1 => {
                // Copy resume command
                let cmd = match session.resume_command.as_deref() {
                    Some(c) => c,
                    None => {
                        println!("{}", "No resume command available.".red());
                        wait_for_enter();
                        continue;
                    }
                };
                let full_cmd = match session.project_dir.as_deref() {
                    Some(dir) if !dir.trim().is_empty() => format!("cd \"{}\" && {}", dir, cmd),
                    _ => cmd.to_string(),
                };
                match copy_to_clipboard(&full_cmd) {
                    Ok(_) => println!("{} {}", "✓".bright_green(), "Copied to clipboard".bold()),
                    Err(e) => println!("{} {}", "✗".red(), e),
                }
                wait_for_enter();
            }
            2 => {
                // Launch in terminal
                let cmd = match session.resume_command.as_deref() {
                    Some(c) => c,
                    None => {
                        println!("{}", "No resume command available.".red());
                        wait_for_enter();
                        continue;
                    }
                };
                match launch_terminal(cmd, session.project_dir.as_deref()) {
                    Ok(_) => println!("{} {}", "✓".bright_green(), "Launched in terminal".bold()),
                    Err(e) => {
                        println!("{} Term launch failed: {}", "✗".yellow(), e);
                        let full_cmd = match session.project_dir.as_deref() {
                            Some(dir) if !dir.trim().is_empty() => format!("cd \"{}\" && {}", dir, cmd),
                            _ => cmd.to_string(),
                        };
                        let _ = copy_to_clipboard(&full_cmd);
                        println!("{} Copied to clipboard as fallback", "✓".bright_green());
                    }
                }
                wait_for_enter();
            }
            3 => {
                // Delete
                let confirmed = Confirm::with_theme(&ColorfulTheme::default())
                    .with_prompt(format!(
                        "Delete session '{}'? This cannot be undone.",
                        format_session_title(session)
                    ))
                    .default(false)
                    .interact()
                    .unwrap_or(false);

                if confirmed {
                    let source_path = match session.source_path.as_deref() {
                        Some(p) => p,
                        None => {
                            println!("{}", "Source path unknown.".red());
                            wait_for_enter();
                            continue;
                        }
                    };
                    match delete_session(&session.provider_id, &session.session_id, source_path) {
                        Ok(true) => {
                            println!("{}", "✓ Session deleted.".bright_green());
                            wait_for_enter();
                            return; // Go back to list
                        }
                        Ok(false) => println!("{}", "Session was not deleted.".yellow()),
                        Err(e) => println!("{} {}", "✗".red(), e),
                    }
                    wait_for_enter();
                }
            }
            _ => return, // Back
        }
    }
}

fn view_messages(session: &SessionMeta) {
    use console::Term;

    let term = Term::stdout();
    let source_path = match session.source_path.as_deref() {
        Some(p) => p,
        None => {
            println!("{}", "No source path available.".red());
            wait_for_enter();
            return;
        }
    };

    let messages = match load_messages(&session.provider_id, source_path) {
        Ok(m) => m,
        Err(e) => {
            println!("{} Failed to load: {}", "✗".red(), e);
            wait_for_enter();
            return;
        }
    };

    if messages.is_empty() {
        println!("  {}", "(no messages)".dimmed());
        wait_for_enter();
        return;
    }

    println!(
        "{}  {}  {}",
        "─".repeat(4).dimmed(),
        format!("{} messages", messages.len()).bold(),
        "─".repeat(50).dimmed()
    );
    println!();

    // Paginate — show in chunks of 20
    let chunk_size = 20;
    let total_chunks = (messages.len() + chunk_size - 1) / chunk_size;

    for (chunk_idx, chunk) in messages.chunks(chunk_size).enumerate() {
        if chunk_idx > 0 {
            println!(
                "\n{} {}/{} {}",
                "───".dimmed(),
                chunk_idx + 1,
                total_chunks,
                "───".dimmed()
            );
            println!("{}", "Press Enter for next page, q to quit.".dimmed());

            let mut input = String::new();
            std::io::stdin().read_line(&mut input).unwrap_or_default();
            if input.trim().to_lowercase() == "q" {
                return;
            }
            let _ = term.clear_screen();
        }

        for msg in chunk {
            let role_display = match msg.role.as_str() {
                "user" => format!("{}", "You".bright_white().bold()),
                "assistant" => format!("{}", "AI".bright_blue().bold()),
                "system" => format!("{}", "Sys".bright_yellow().bold()),
                "tool" => format!("{}", "Tool".bright_magenta().bold()),
                _ => msg.role.clone(),
            };
            let ts_str = msg.ts.map(format_timestamp).unwrap_or_default();

            println!(
                "{} {} {}",
                role_display,
                ts_str.dimmed(),
                "─".repeat(30).dimmed()
            );

            // Truncate very long tool outputs
            let content = &msg.content;
            let lines: Vec<&str> = content.lines().collect();
            if lines.len() > 30 {
                for line in lines.iter().take(25) {
                    println!("  {}", line);
                }
                println!(
                    "  {}",
                    format!("... ({} more lines)", lines.len() - 25).dimmed()
                );
            } else {
                for line in &lines {
                    println!("  {}", line);
                }
            }
            println!();
        }
    }

    println!();
    wait_for_enter();
}

fn wait_for_enter() {
    use std::io::Write;
    println!();
    println!("{}", "Press Enter to continue...".dimmed());
    let _ = std::io::stdout().flush();
    let mut buf = String::new();
    let _ = std::io::stdin().read_line(&mut buf);
}

// ═══════════════════════════════════════════════════════════
//  CLI COMMANDS
// ═══════════════════════════════════════════════════════════

fn cmd_list(provider: Option<&str>, search: Option<&str>, limit: Option<usize>) {
    let all = scan_all();
    let sessions = filter_sessions(all, provider, search);
    let limit = limit.unwrap_or(usize::MAX);

    if sessions.is_empty() {
        println!(
            "{}",
            "No sessions found. Try another provider or search term.".yellow()
        );
        return;
    }

    // Stats line
    let mut counts: Vec<String> = Vec::new();
    let (mut oc, mut cc, mut cx) = (0, 0, 0);
    for s in &sessions {
        match s.provider_id.as_str() {
            "opencode" => oc += 1,
            "claude" => cc += 1,
            "codex" => cx += 1,
            _ => {}
        }
    }
    if oc > 0 { counts.push(format!("{} OpenCode", oc)); }
    if cc > 0 { counts.push(format!("{} Claude", cc)); }
    if cx > 0 { counts.push(format!("{} Codex", cx)); }
    println!("{} {}", "Sessions:".bright_cyan().bold(), counts.join("  "));
    println!();

    let _max = sessions.len().min(limit);
    for (i, s) in sessions.iter().take(limit).enumerate() {
        let icon = provider_icon(&s.provider_id);
        let id = shorten_id(&s.session_id, 36);
        let title = format_session_title(s);
        let project = s
            .project_dir
            .as_deref()
            .and_then(utils::path_basename)
            .unwrap_or_default();
        let time = s
            .last_active_at
            .or(s.created_at)
            .map(|t| format_timestamp(t))
            .unwrap_or_default();

        let num = format!("{}.", i + 1).dimmed();
        println!(
            "{} {}  {}  {}",
            num,
            icon,
            trunc(&title, 55).bright_white(),
            format!("{}  {}", time.dimmed(), project.dimmed())
        );
        println!(
            "   {} {}",
            "id:".dimmed(),
            id.dimmed()
        );
    }

    if sessions.len() > limit {
        println!(
            "\n  ... and {} more (use --limit to increase)",
            sessions.len() - limit
        );
    }
}

fn cmd_show(id: &str, provider: Option<&str>, search: Option<&str>) {
    let sessions = scan_all();
    let session = match find_session(&sessions, id, provider) {
        Some(s) => s,
        None => {
            eprintln!("{} Session '{}' not found.", "✗".red(), id);
            std::process::exit(1);
        }
    };

    let source_path = match session.source_path.as_deref() {
        Some(p) => p,
        None => {
            eprintln!("{} No source path available.", "✗".red());
            std::process::exit(1);
        }
    };

    let messages = match load_messages(&session.provider_id, source_path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{} {}", "✗".red(), e);
            std::process::exit(1);
        }
    };

    // Filter messages by search if provided
    let filtered: Vec<&_> = if let Some(q) = search {
        let q = q.to_lowercase();
        messages
            .iter()
            .filter(|m| m.content.to_lowercase().contains(&q))
            .collect()
    } else {
        messages.iter().collect()
    };

    // Print header
    let header = format!(
        "{}  {}  {} messages  {}",
        provider_icon(&session.provider_id),
        format_session_title(session),
        messages.len(),
        format!(
            "id:{}",
            shorten_id(&session.session_id, 32)
        )
        .dimmed()
    );
    println!("\n{}", header);
    if let Some(ref cmd) = session.resume_command {
        println!("{} {}", "Resume:".dimmed(), cmd.bright_green());
    }
    println!("{}", "─".repeat(70).dimmed());
    println!();

    if filtered.is_empty() {
        println!("  {}", "(no matching messages)".dimmed());
        return;
    }

    for msg in &filtered {
        let role = match msg.role.as_str() {
            "user" => format!("{}", "You".bright_white().bold()),
            "assistant" => format!("{}", "AI".bright_blue().bold()),
            "system" => format!("{}", "Sys".bright_yellow().bold()),
            "tool" => format!("{}", "Tool".bright_magenta().bold()),
            _ => msg.role.clone(),
        };
        let ts = msg.ts.map(format_timestamp).unwrap_or_default();

        println!("{} {} {}", role, ts.dimmed(), "─".repeat(30).dimmed());

        let lines: Vec<&str> = msg.content.lines().collect();
        let display_lines = if lines.len() > 40 {
            lines.iter().take(35).copied().collect::<Vec<_>>()
        } else {
            lines
        };
        for line in &display_lines {
            println!("  {}", line);
        }
        if msg.content.lines().count() > 40 {
            println!(
                "  {}",
                format!("... ({} more lines)", msg.content.lines().count() - 35)
                    .dimmed()
            );
        }
        println!();
    }
}

fn cmd_resume(id: &str, provider: Option<&str>, copy_only: bool, launch: bool) {
    let sessions = scan_all();
    let session = match find_session(&sessions, id, provider) {
        Some(s) => s,
        None => {
            eprintln!("{} Session '{}' not found.", "✗".red(), id);
            std::process::exit(1);
        }
    };

    let cmd = match session.resume_command.as_deref() {
        Some(c) => c,
        None => {
            eprintln!("{} No resume command available.", "✗".red());
            std::process::exit(1);
        }
    };

    let full_cmd = match session.project_dir.as_deref() {
        Some(dir) if !dir.trim().is_empty() => format!("cd \"{}\" && {}", dir, cmd),
        _ => cmd.to_string(),
    };

    println!(
        "\n{}  {}",
        provider_icon(&session.provider_id),
        format_session_title(session).bold()
    );
    println!("{} {}", "Command:".dimmed(), full_cmd.bright_green());

    if copy_only || !launch {
        match copy_to_clipboard(&full_cmd) {
            Ok(_) => println!("{} {}", "✓".bright_green(), "Copied to clipboard"),
            Err(e) => {
                println!("{} {}", "✗".yellow(), e);
                println!("\n  Run this:\n  {}", full_cmd);
            }
        }
    } else {
        match launch_terminal(cmd, session.project_dir.as_deref()) {
            Ok(_) => println!("{} {}", "✓".bright_green(), "Launched in terminal"),
            Err(e) => {
                println!("{} Term failed: {}", "✗".yellow(), e);
                let _ = copy_to_clipboard(&full_cmd);
                println!("{} Copied to clipboard instead", "✓".bright_green());
            }
        }
    }
}

fn cmd_delete(id: &str, provider: Option<&str>, force: bool) {
    let sessions = scan_all();
    let session = match find_session(&sessions, id, provider) {
        Some(s) => s,
        None => {
            eprintln!("{} Session '{}' not found.", "✗".red(), id);
            std::process::exit(1);
        }
    };

    let source_path = match session.source_path.as_deref() {
        Some(p) => p,
        None => {
            eprintln!("{} Source path unknown.", "✗".red());
            std::process::exit(1);
        }
    };

    let title = format_session_title(session);

    if !force {
        use dialoguer::{theme::ColorfulTheme, Confirm};
        let confirmed = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(format!(
                "Delete '{}'? This cannot be undone.",
                title
            ))
            .default(false)
            .interact()
            .unwrap_or(false);

        if !confirmed {
            println!("{}", "Cancelled.".dimmed());
            return;
        }
    }

    match delete_session(&session.provider_id, &session.session_id, source_path) {
        Ok(true) => println!("{} Deleted '{}'", "✓".bright_green(), title),
        Ok(false) => println!("{} Session was not deleted.", "✗".yellow()),
        Err(e) => eprintln!("{} {}", "✗".red(), e),
    }
}

fn cmd_info() {
    println!("{}", BANNER.bright_cyan());
    println!("{}", "Data Sources".bold().underline());
    println!();

    // OpenCode
    println!("{}", "OpenCode".bright_cyan().bold());
    let oc_dirs = utils::opencode_base_dirs();
    for dir in &oc_dirs {
        let (marker, style) = if dir.exists() {
            ("✓ found", "bright_green")
        } else {
            ("✗ not found", "red")
        };
        println!(
            "  {}  {}",
            match style {
                "bright_green" => marker.bright_green(),
                _ => marker.red(),
            },
            dir.display().to_string().dimmed()
        );
        if dir.exists() {
            let db = dir.join("opencode.db");
            if db.exists() {
                println!("    {} opencode.db (SQLite mode)", "↳".dimmed());
            }
            let storage = dir.join("storage");
            if storage.exists() {
                println!("    {} storage/  (JSON mode)", "↳".dimmed());
            }
        }
    }
    println!();

    // Claude Code
    println!("{}", "Claude Code".bright_yellow().bold());
    let cc_dirs = utils::claude_projects_dirs();
    for dir in &cc_dirs {
        let count = count_jsonl_files(dir);
        let (marker, style) = if dir.exists() {
            ("✓ found", "bright_green")
        } else {
            ("✗ not found", "red")
        };
        println!(
            "  {}  {}  ({})",
            match style {
                "bright_green" => marker.bright_green(),
                _ => marker.red(),
            },
            dir.display().to_string().dimmed(),
            format!("{} files", count).dimmed()
        );
    }
    println!();

    // Codex
    println!("{}", "Codex".bright_green().bold());
    let cx_dirs = utils::codex_sessions_dirs();
    for dir in &cx_dirs {
        let count = count_jsonl_files(dir);
        let (marker, style) = if dir.exists() {
            ("✓ found", "bright_green")
        } else {
            ("✗ not found", "red")
        };
        println!(
            "  {}  {}  ({})",
            match style {
                "bright_green" => marker.bright_green(),
                _ => marker.red(),
            },
            dir.display().to_string().dimmed(),
            format!("{} files", count).dimmed()
        );
    }
    println!();

    // Session counts
    let all = scan_all();
    let (mut oc, mut cc, mut cx) = (0, 0, 0);
    for s in &all {
        match s.provider_id.as_str() {
            "opencode" => oc += 1,
            "claude" => cc += 1,
            "codex" => cx += 1,
            _ => {}
        }
    }
    println!("{}", "Session Totals".bold().underline());
    println!("  OpenCode:    {}", oc.to_string().bright_cyan().bold());
    println!("  Claude Code: {}", cc.to_string().bright_yellow().bold());
    println!("  Codex:       {}", cx.to_string().bright_green().bold());
    println!(
        "  Total:       {}",
        all.len().to_string().bright_white().bold()
    );
    println!();
}

// ═══════════════════════════════════════════════════════════
//  HELPERS
// ═══════════════════════════════════════════════════════════

fn filter_sessions(
    sessions: Vec<SessionMeta>,
    provider: Option<&str>,
    search: Option<&str>,
) -> Vec<SessionMeta> {
    sessions
        .into_iter()
        .filter(|s| {
            if let Some(ref p) = provider {
                if s.provider_id != p.to_lowercase() {
                    return false;
                }
            }
            if let Some(ref q) = search {
                let q = q.to_lowercase();
                let title_match = s
                    .title
                    .as_deref()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(&q);
                let summary_match = s
                    .summary
                    .as_deref()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(&q);
                let dir_match = s
                    .project_dir
                    .as_deref()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(&q);
                if !title_match && !summary_match && !dir_match {
                    return false;
                }
            }
            true
        })
        .collect()
}

fn find_session<'a>(
    sessions: &'a [SessionMeta],
    id: &str,
    provider: Option<&str>,
) -> Option<&'a SessionMeta> {
    sessions.iter().find(|s| {
        let id_match = s.session_id == id
            || s.session_id.starts_with(id)
            || s.session_id.to_lowercase() == id.to_lowercase();
        let provider_match =
            provider.map_or(true, |p| s.provider_id.to_lowercase() == p.to_lowercase());
        id_match && provider_match
    })
}

fn provider_icon(provider_id: &str) -> String {
    match provider_id {
        "opencode" => "[OC]".bright_cyan().to_string(),
        "claude" => "[CL]".bright_yellow().to_string(),
        "codex" => "[CX]".bright_green().to_string(),
        _ => format!("[{}]", &provider_id[..2.min(provider_id.len())]),
    }
}

fn provider_label(provider_id: &str) -> &str {
    match provider_id {
        "opencode" => "OpenCode",
        "claude" => "Claude Code",
        "codex" => "Codex",
        _ => provider_id,
    }
}

fn format_session_title(session: &SessionMeta) -> String {
    session.title.clone().unwrap_or_else(|| {
        session
            .project_dir
            .as_deref()
            .and_then(utils::path_basename)
            .unwrap_or_else(|| session.session_id.clone())
    })
}

fn shorten_id(id: &str, max_len: usize) -> String {
    if id.len() <= max_len {
        return id.to_string();
    }
    format!("{}...", &id[..max_len.saturating_sub(3)])
}

fn trunc(s: &str, max_chars: usize) -> String {
    let trimmed = s.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let mut result: String = trimmed.chars().take(max_chars.saturating_sub(3)).collect();
    result.push_str("...");
    result
}

fn count_jsonl_files(dir: &std::path::Path) -> usize {
    if !dir.exists() {
        return 0;
    }
    let mut count = 0;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        if let Ok(entries) = std::fs::read_dir(&current) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                    count += 1;
                }
            }
        }
    }
    count
}
