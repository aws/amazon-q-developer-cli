//! /code command execution — status, init, logs, overview, summary

use agent::tui_commands::{
    CodeArgs,
    CommandResult,
};
use code_agent_sdk::sdk::WorkspaceStatus;
use serde_json::json;

use super::CommandContext;

pub async fn execute(args: &CodeArgs, ctx: &CommandContext<'_>) -> CommandResult {
    let sub = args.subcommand.as_deref().unwrap_or("status");
    let (cmd, rest) = sub.split_once(' ').unwrap_or((sub, ""));
    match cmd {
        "status" => execute_status(ctx, false).await,
        "init" => execute_status(ctx, true).await,
        "logs" => {
            let parts: Vec<&str> = rest.split_whitespace().collect();
            let mut level = "ERROR";
            let mut lines: usize = 20;
            for pair in parts.chunks(2) {
                match pair {
                    ["-l" | "--level", val] => level = val,
                    ["-n" | "--lines", val] => lines = val.parse().unwrap_or(20),
                    _ => {},
                }
            }
            execute_logs(level, lines)
        },
        "overview" => execute_overview(ctx, false).await,
        "summary" => execute_overview(ctx, true).await,
        _ => CommandResult::error(format!(
            "Unknown subcommand '{}'. Use: status, init, logs, overview, summary",
            cmd
        )),
    }
}

async fn execute_status(ctx: &CommandContext<'_>, initialize: bool) -> CommandResult {
    let Some(ci) = ctx.session_tx.get_code_intelligence(ctx.cwd.to_path_buf()).await else {
        return CommandResult::success_with_data(
            String::new(),
            json!({
                "status": "unavailable",
                "message": "Code intelligence not available.",
            }),
        );
    };

    let mut client = ci.write().await;

    // Auto-initialize if requested
    let mut init_message = None;
    let mut warning = None;
    if initialize {
        match client.workspace_status() {
            WorkspaceStatus::NotInitialized => {
                // Warn if initializing home directory
                if let Some(home) = dirs::home_dir() {
                    let ws_root = client.workspace_manager_mut().workspace_root().to_path_buf();
                    if home.canonicalize().ok() == ws_root.canonicalize().ok() {
                        warning = Some(
                            "Workspace is home directory - scan depth will be limited. If initialized by mistake, remove ~/.kiro/settings/lsp.json",
                        );
                    }
                }
                match client.initialize().await {
                    Ok(_) => init_message = Some("Workspace initialization started"),
                    Err(e) => return CommandResult::error(format!("Failed to initialize workspace: {e}")),
                }
            },
            _ => init_message = Some("Workspace already initialized"),
        }
    }

    let status = match client.workspace_status() {
        WorkspaceStatus::NotInitialized => "not_initialized",
        WorkspaceStatus::Initializing => "initializing",
        WorkspaceStatus::Initialized => "initialized",
    };

    // Gather workspace info
    let workspace_info = client.detect_workspace().ok();
    let mut lsps = Vec::new();
    let mut detected_languages = Vec::new();
    let mut root_path = String::new();
    let mut project_markers = Vec::new();

    if let Some(info) = &workspace_info {
        root_path = info.root_path.display().to_string();
        detected_languages = info.detected_languages.clone();
        project_markers = info.project_markers.clone();

        let detected_set: std::collections::HashSet<&str> =
            info.detected_languages.iter().map(|s| s.as_str()).collect();
        let workspace_initialized = client.workspace_status() == WorkspaceStatus::Initialized;

        for lsp in &info.available_lsps {
            let is_relevant = lsp.languages.iter().any(|l| detected_set.contains(l.as_str()));
            let lsp_status = match lsp.status.as_deref() {
                Some("initialized") => "initialized",
                Some("initializing" | "registered") if lsp.is_available && is_relevant => "initializing",
                Some(s) if s.starts_with("failed:") => "failed",
                _ if lsp.is_initialized => "initialized",
                _ if lsp.is_available && is_relevant && workspace_initialized => "failed",
                _ if !lsp.is_available => "not_installed",
                _ => "available",
            };

            lsps.push(json!({
                "name": lsp.name,
                "languages": lsp.languages,
                "status": lsp_status,
                "isAvailable": lsp.is_available,
                "initDurationMs": lsp.init_duration_ms,
                "workspaceFolders": lsp.workspace_folders,
            }));
        }
    }

    let status_message = init_message.unwrap_or(match status {
        "not_initialized" => "Workspace not initialized. Run /code init to initialize.",
        "initializing" => "Workspace initialization in progress",
        "initialized" => "Workspace initialized",
        _ => "",
    });

    CommandResult::success_with_data(
        String::new(),
        json!({
            "status": status,
            "message": status_message,
            "warning": warning,
            "rootPath": root_path,
            "detectedLanguages": detected_languages,
            "projectMarkers": project_markers,
            "lsps": lsps,
            "configPath": ".kiro/settings/lsp.json",
            "docUrl": "https://kiro.dev/docs/cli/code-intelligence/",
        }),
    )
}

async fn execute_overview(ctx: &CommandContext<'_>, is_summary: bool) -> CommandResult {
    let Some(ci) = ctx.session_tx.get_code_intelligence(ctx.cwd.to_path_buf()).await else {
        return CommandResult::error("Code intelligence not available.".to_string());
    };

    let mut client = ci.write().await;

    // Auto-initialize if needed
    if client.workspace_status() == WorkspaceStatus::NotInitialized {
        if client.should_auto_initialize() {
            if let Err(e) = client.initialize().await {
                return CommandResult::error(format!("Failed to initialize workspace: {e}"));
            }
        } else {
            return CommandResult::error("Workspace not initialized. Run /code init first.".to_string());
        }
    }

    let request = code_agent_sdk::model::types::GenerateCodebaseOverviewRequest {
        path: None,
        timeout_secs: None,
        token_budget: None,
    };

    match client.generate_codebase_overview(request).await {
        Ok(overview) => {
            let overview_json = serde_json::to_string(&overview).unwrap_or_default();

            let prompt = if is_summary {
                let sop = include_str!("codebase-summary.sop.md");
                format!(
                    "{sop}\n\nHere is the codebase overview:\n\n{overview_json}\n\nYou MUST rely on the overview information when possible and only dive deeper into the codebase if necessary.\n\nAnalyze this codebase and help me create comprehensive documentation. Ask me for the parameters you need to proceed.\n\nWhen presenting options, use lettered choices (a, b, c) and end with: \"(Reply with your choices, e.g., '1=a, 2=b' or provide custom preferences)\""
                )
            } else {
                format!(
                    "Here is the codebase overview:\n\n{overview_json}\n\nAnalyze this and summarize the key components, architecture, and entry points.\n\nWhen answering questions: You MUST answer FIRST using ONLY the information available in the overview above. You MUST NOT use any tools unless the user explicitly asks for more details or you cannot answer from the overview. If exploration is needed, you SHOULD ask the user before proceeding.\n\nIf you must explore: You MUST perform small, focused searches with small limits (e.g., limit=5-10, not 50). Extract minimal information only.\n\nAvailable tools (use sparingly):\n- search_symbols, get_document_symbols, search_codebase_map, fs_read, pattern_search\n\nLSP tools MAY be available: find_references, goto_definition, get_hover"
                )
            };

            let tokens = overview_json.len() / 4;
            let label = if is_summary {
                format!("/code summary · overview generated (~{tokens} tokens)")
            } else {
                format!("/code overview · overview generated (~{tokens} tokens)")
            };

            CommandResult::success_with_data(String::new(), json!({ "executePrompt": prompt, "label": label }))
        },
        Err(e) => CommandResult::error(format!("Failed to generate codebase overview: {e}")),
    }
}

fn execute_logs(level: &str, lines: usize) -> CommandResult {
    if level_priority(level) == 0 {
        return CommandResult::error(format!(
            "Invalid log level '{}'. Valid levels: ERROR, WARN, INFO, DEBUG, TRACE",
            level
        ));
    }

    let log_path = match crate::util::paths::logs_dir() {
        Ok(dir) => dir.join("lsp.log"),
        Err(e) => return CommandResult::error(format!("Failed to get logs dir: {e}")),
    };

    if !log_path.exists() {
        return CommandResult::success_with_data(String::new(), json!({ "entries": [], "level": level }));
    }

    let content = match std::fs::read_to_string(&log_path) {
        Ok(c) => c,
        Err(e) => return CommandResult::error(format!("Failed to read log file: {e}")),
    };

    let min_level = level_priority(level);
    let entries: Vec<serde_json::Value> = content
        .lines()
        .filter_map(parse_log_line)
        .filter(|(_, lvl, _)| level_priority(lvl) >= min_level)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .take(lines)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|(ts, lvl, msg)| json!({ "timestamp": ts, "level": lvl, "message": msg }))
        .collect();

    CommandResult::success_with_data(String::new(), json!({ "entries": entries, "level": level }))
}

fn parse_log_line(line: &str) -> Option<(String, String, String)> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let clean = strip_ansi(line);
    let parts: Vec<&str> = clean.splitn(4, ' ').collect();
    if parts.len() < 4 {
        return None;
    }
    let timestamp = parts[0].to_string();
    let level = parts[1].to_string();
    let rest = parts[3..].join(" ");

    let mut message = rest.clone();
    let bytes = rest.as_bytes();
    for i in 0..bytes.len().saturating_sub(4) {
        if bytes[i] == b':' && bytes[i + 1] == b' ' {
            let mut j = i + 2;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if j > i + 2 && j + 1 < bytes.len() && bytes[j] == b':' && bytes[j + 1] == b' ' {
                message = rest[j + 2..].to_string();
                break;
            }
        }
    }
    Some((timestamp, level, message))
}

fn strip_ansi(s: &str) -> String {
    let mut result = String::new();
    let mut in_escape = false;
    for c in s.chars() {
        if c == '\x1b' {
            in_escape = true;
            continue;
        }
        if in_escape {
            if c == 'm' {
                in_escape = false;
            }
            continue;
        }
        result.push(c);
    }
    result
}

fn level_priority(level: &str) -> u8 {
    match level.to_uppercase().as_str() {
        "ERROR" => 5,
        "WARN" => 4,
        "INFO" => 3,
        "DEBUG" => 2,
        "TRACE" => 1,
        _ => 0,
    }
}
