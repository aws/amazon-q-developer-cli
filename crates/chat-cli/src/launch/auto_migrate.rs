//! Auto-migration of V2 agent configs to the universal format for the KAS engine.
//!
//! Runs before the TUI/KAS process spawns so configs are on disk in a format KAS can load.
//! Interactive mode prompts the user (migrate now / not now / don't ask again) and migrates all
//! pending V2-only configs on confirmation. Non-interactive mode silently migrates only the target
//! agent (from `--agent` or `chat.defaultAgent`).

use std::path::Path;

use agent::agent_config::migration::{
    AgentClassification,
    AgentScope,
    ScannedAgent,
    default_scan_dirs,
    scan_agents,
    upgrade_agent_file,
};
use agent::util::providers::RealProvider;
use tracing::info;

/// The action to take after the interactive migration prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PromptAction {
    Migrate,
    Skip,
    DisableAndSkip,
}

/// Map a dialoguer selection index to a [`PromptAction`]. `None` (user cancelled or I/O error)
/// maps to `Skip`.
pub(super) fn selection_to_action(selection: Option<usize>) -> PromptAction {
    match selection {
        Some(0) => PromptAction::Migrate,
        Some(2) => PromptAction::DisableAndSkip,
        _ => PromptAction::Skip,
    }
}

/// Select which agents to migrate in non-interactive mode: only the named target agent.
pub(super) fn select_non_interactive_migration_targets<'a>(
    pending: &[&'a ScannedAgent],
    target_agent: Option<&str>,
) -> Vec<&'a ScannedAgent> {
    let Some(name) = target_agent else {
        return Vec::new();
    };
    pending.iter().filter(|a| a.name == name).copied().collect()
}

/// Filter scan results to only V2-only agents that need migration.
pub(super) fn pending_migrations(agents: &[ScannedAgent]) -> Vec<&ScannedAgent> {
    agents
        .iter()
        .filter(|a| a.classification == AgentClassification::V2Only)
        .collect()
}

/// Auto-migrate V2 agent configs to the universal format before KAS reads them.
/// In interactive mode, shows agent count by scope and prompts to migrate.
/// In non-interactive mode, silently migrates only the target agent (from `--agent`
/// or `chat.defaultAgent`).
/// Returns `true` if the user chose "Don't ask again" (caller should persist the setting).
pub fn auto_migrate_agent_configs(
    interactive: bool,
    target_agent: Option<&str>,
    database: &crate::database::Database,
) -> bool {
    if database
        .settings
        .get_bool(crate::database::settings::Setting::ChatDisableAutoAgentUpgrade)
        .unwrap_or(false)
    {
        return false;
    }

    let dirs = default_scan_dirs(&RealProvider);
    let scan = scan_agents(&dirs);

    let pending = pending_migrations(&scan.agents);

    if pending.is_empty() {
        return false;
    }

    let to_migrate: Vec<_> = if interactive {
        let local: Vec<&str> = pending
            .iter()
            .filter(|a| a.scope == AgentScope::Local)
            .map(|a| a.name.as_str())
            .collect();
        let global: Vec<&str> = pending
            .iter()
            .filter(|a| a.scope == AgentScope::Global)
            .map(|a| a.name.as_str())
            .collect();

        let scope_summary = match (local.len(), global.len()) {
            (l, g) if l > 0 && g > 0 => format!("{l} workspace and {g} global"),
            (l, _) if l > 0 => format!("{l} workspace"),
            (_, g) => format!("{g} global"),
        };
        eprintln!("V2-only agent configs ({scope_summary}) need migration to work with the V3 engine.");

        let options = &["Yes, migrate now", "Not now", "Don't ask again"];
        let selection = match dialoguer::Select::with_theme(&crate::util::dialoguer_theme())
            .with_prompt("Migrate to universal format? (originals backed up to .bak)")
            .items(options)
            .default(0)
            .interact_on_opt(&dialoguer::console::Term::stderr())
        {
            Ok(selection) => selection,
            Err(e) => {
                tracing::warn!("Agent migration prompt failed, skipping: {e}");
                None
            },
        };
        match selection_to_action(selection) {
            PromptAction::Migrate => {},
            PromptAction::DisableAndSkip => return true,
            PromptAction::Skip => return false,
        }
        pending
    } else {
        let targets = select_non_interactive_migration_targets(&pending, target_agent);
        if targets.is_empty() {
            return false;
        }
        targets
    };

    for agent in &to_migrate {
        let outcome = upgrade_agent_file(Path::new(&agent.source_path));
        match outcome.status {
            agent::agent_config::migration::UpgradeStatus::Upgraded => {
                info!(
                    agent = %agent.name,
                    path = %agent.source_path,
                    "Auto-migrated agent config to universal format"
                );
            },
            agent::agent_config::migration::UpgradeStatus::Error => {
                tracing::warn!(
                    agent = %agent.name,
                    path = %agent.source_path,
                    error = ?outcome.error,
                    "Failed to auto-migrate agent config"
                );
            },
            _ => {},
        }
    }
    false
}

/// Persist the `chat.disableAutoAgentUpgrade` opt-out to the workspace settings file at `path`.
/// The caller resolves `path` via the canonical `PathResolver` so this stays in sync with where
/// `Settings` reads from. Preserves existing keys; bails out without writing if the file exists but
/// can't be parsed, so a transient syntax error never clobbers the user's settings.
pub fn persist_disable_setting(path: &Path) {
    if let Err(e) = write_disable_setting(path) {
        tracing::warn!("Failed to save auto-migration opt-out: {e}");
    }
}

/// Insert the opt-out key into the settings file at `path`, preserving existing keys. Errors
/// (rather than clobbering) if the file exists but isn't a readable JSON object.
fn write_disable_setting(path: &Path) -> Result<(), String> {
    use crate::database::settings::Setting;

    let mut settings: serde_json::Map<String, serde_json::Value> = if path.exists() {
        let raw = std::fs::read_to_string(path).map_err(|e| format!("read: {e}"))?;
        match serde_json::from_str(&raw).map_err(|e| format!("parse: {e}"))? {
            serde_json::Value::Object(map) => map,
            _ => return Err("settings file is not a JSON object".to_string()),
        }
    } else {
        serde_json::Map::new()
    };

    settings.insert(
        Setting::ChatDisableAutoAgentUpgrade.as_ref().to_string(),
        serde_json::Value::Bool(true),
    );

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir: {e}"))?;
    }
    let mut serialized = serde_json::to_string_pretty(&settings).map_err(|e| format!("serialize: {e}"))?;
    serialized.push('\n');
    std::fs::write(path, serialized).map_err(|e| format!("write: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_agent(name: &str, scope: AgentScope, classification: AgentClassification) -> ScannedAgent {
        ScannedAgent {
            name: name.to_string(),
            scope,
            classification,
            source_path: format!("/fake/{name}.json"),
            warnings: Vec::new(),
        }
    }

    #[test]
    fn non_interactive_selects_only_target_agent() {
        let agents = vec![
            make_agent("docs", AgentScope::Local, AgentClassification::V2Only),
            make_agent("oncall", AgentScope::Local, AgentClassification::V2Only),
            make_agent("helper", AgentScope::Global, AgentClassification::V2Only),
        ];
        let pending: Vec<&ScannedAgent> = agents.iter().collect();

        let result = select_non_interactive_migration_targets(&pending, Some("oncall"));
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "oncall");
    }

    #[test]
    fn non_interactive_no_target_returns_empty() {
        let agents = vec![make_agent("docs", AgentScope::Local, AgentClassification::V2Only)];
        let pending: Vec<&ScannedAgent> = agents.iter().collect();

        let result = select_non_interactive_migration_targets(&pending, None);
        assert!(result.is_empty());
    }

    #[test]
    fn non_interactive_unknown_target_returns_empty() {
        let agents = vec![make_agent("docs", AgentScope::Local, AgentClassification::V2Only)];
        let pending: Vec<&ScannedAgent> = agents.iter().collect();

        let result = select_non_interactive_migration_targets(&pending, Some("nonexistent"));
        assert!(result.is_empty());
    }

    #[test]
    fn non_interactive_matches_by_name_across_scopes() {
        let agents = vec![
            make_agent("shared", AgentScope::Local, AgentClassification::V2Only),
            make_agent("shared", AgentScope::Global, AgentClassification::V2Only),
        ];
        let pending: Vec<&ScannedAgent> = agents.iter().collect();

        let result = select_non_interactive_migration_targets(&pending, Some("shared"));
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn pending_migrations_includes_only_v2_only() {
        let agents = vec![
            make_agent("v2", AgentScope::Local, AgentClassification::V2Only),
            make_agent("oos", AgentScope::Local, AgentClassification::UniversalOutOfSync),
            make_agent("synced", AgentScope::Local, AgentClassification::UniversalInSync),
            make_agent("v3", AgentScope::Global, AgentClassification::V3Only),
            make_agent("v2g", AgentScope::Global, AgentClassification::V2Only),
        ];

        let result = pending_migrations(&agents);
        let names: Vec<&str> = result.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, vec!["v2", "v2g"]);
    }

    #[test]
    fn selection_maps_to_action() {
        assert_eq!(selection_to_action(Some(0)), PromptAction::Migrate);
        assert_eq!(selection_to_action(Some(1)), PromptAction::Skip);
        assert_eq!(selection_to_action(Some(2)), PromptAction::DisableAndSkip);
        // Cancel / I/O error → skip (never migrate, never disable).
        assert_eq!(selection_to_action(None), PromptAction::Skip);
    }

    #[test]
    fn write_disable_setting_creates_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(".kiro").join("settings").join("cli.json");

        write_disable_setting(&path).expect("write");

        let raw = std::fs::read_to_string(&path).expect("read");
        assert!(raw.ends_with('\n'), "settings file should end with a newline");
        let value: serde_json::Value = serde_json::from_str(&raw).expect("parse");
        assert_eq!(value["chat.disableAutoAgentUpgrade"], serde_json::Value::Bool(true));
    }

    #[test]
    fn write_disable_setting_preserves_existing_keys() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("cli.json");
        std::fs::write(&path, r#"{"chat.defaultModel": "claude", "other": 42}"#).expect("seed");

        write_disable_setting(&path).expect("write");

        let value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("parse");
        assert_eq!(value["chat.defaultModel"], "claude");
        assert_eq!(value["other"], 42);
        assert_eq!(value["chat.disableAutoAgentUpgrade"], serde_json::Value::Bool(true));
    }

    #[test]
    fn write_disable_setting_refuses_to_clobber_malformed_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("cli.json");
        std::fs::write(&path, "{ not valid json").expect("seed");

        // Must error rather than overwrite — the user's file is left untouched.
        assert!(write_disable_setting(&path).is_err());
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "{ not valid json");
    }
}
