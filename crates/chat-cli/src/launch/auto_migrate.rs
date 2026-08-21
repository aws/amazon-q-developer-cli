//! Auto-upgrade of V2 agent configs to the universal format for the KAS engine.
//!
//! `resolve_auto_upgrade_consent` reads the `chat.enableAutoAgentUpgrade` setting (prompting once
//! when unset) and reports whether to upgrade; `upgrade_agent_configs` performs the upgrade.

use std::io::IsTerminal;
use std::path::Path;

use agent::agent_config::migration::{
    AgentClassification,
    ScannedAgent,
    default_scan_dirs,
    scan_agents,
    upgrade_agent_file,
};
use agent::util::providers::RealProvider;
use tracing::info;

/// The user's choice at the interactive upgrade prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PromptChoice {
    Enable,
    Decline,
    /// Cancel or I/O error — persist nothing so the prompt returns next launch.
    Dismiss,
}

/// `None` (cancel or I/O error) maps to `Dismiss`, so an escape neither enables nor declines.
pub(super) fn selection_to_choice(selection: Option<usize>) -> PromptChoice {
    match selection {
        Some(0) => PromptChoice::Enable,
        Some(1) => PromptChoice::Decline,
        _ => PromptChoice::Dismiss,
    }
}

/// V2-only agents that still need upgrading.
pub(super) fn pending_migrations(agents: &[ScannedAgent]) -> Vec<&ScannedAgent> {
    agents
        .iter()
        .filter(|a| a.classification == AgentClassification::V2Only)
        .collect()
}

/// Upgrade each pending config in place, logging per-agent outcomes.
fn run_migration(to_migrate: &[&ScannedAgent]) {
    for agent in to_migrate {
        let outcome = upgrade_agent_file(Path::new(&agent.source_path));
        match outcome.status {
            agent::agent_config::migration::UpgradeStatus::Upgraded => {
                info!(
                    agent = %agent.name,
                    path = %agent.source_path,
                    "Auto-upgraded agent config to universal format"
                );
            },
            agent::agent_config::migration::UpgradeStatus::Error => {
                tracing::warn!(
                    agent = %agent.name,
                    path = %agent.source_path,
                    error = ?outcome.error,
                    "Failed to auto-upgrade agent config"
                );
            },
            _ => {},
        }
    }
}

/// Scan and upgrade every pending V2-only config in place. Idempotent.
pub fn upgrade_agent_configs() {
    let scan = scan_agents(&default_scan_dirs(&RealProvider));
    run_migration(&pending_migrations(&scan.agents));
}

/// Whether this launch should upgrade agent configs, capturing consent once. `Some(true)`/
/// `Some(false)` are honored without prompting; when unset, prompt (interactive TTY, and only if
/// configs are pending) and persist an Enable/Decline choice. Never upgrades — the caller does.
pub async fn resolve_auto_upgrade_consent(interactive: bool, database: &crate::database::Database) -> bool {
    use crate::database::settings::Setting;

    if let Some(value) = database.settings.get_bool(Setting::ChatEnableAutoAgentUpgrade) {
        return value;
    }

    // Undecided: only worth prompting if there's something to upgrade.
    let scan = scan_agents(&default_scan_dirs(&RealProvider));
    if pending_migrations(&scan.agents).is_empty() {
        return false;
    }

    // `interactive` is the CLI form, not a TTY check; the prompt renders to stderr, so guard on it too.
    if !interactive || !std::io::stderr().is_terminal() {
        return false;
    }

    eprintln!("You're on Kiro 3.0 — thanks for giving it a try.");
    eprintln!(
        "Your agent configs are still in the 2.0 format. Upgrade them to run on 3.0? They'll keep working in 2.0."
    );
    let options = &["Enable auto-upgrade", "Not now — I'll do it later"];
    let selection = match dialoguer::Select::with_theme(&crate::util::dialoguer_theme())
        .items(options)
        .default(0)
        .interact_on_opt(&dialoguer::console::Term::stderr())
    {
        Ok(selection) => selection,
        Err(e) => {
            tracing::warn!("Agent upgrade prompt failed, skipping: {e}");
            None
        },
    };

    match selection_to_choice(selection) {
        PromptChoice::Enable => {
            persist_consent(&database.settings, true).await;
            true
        },
        PromptChoice::Decline => {
            persist_consent(&database.settings, false).await;
            false
        },
        PromptChoice::Dismiss => false,
    }
}

/// Clones the in-memory settings because the launch path holds `&Os`, not `&mut`. A failed write
/// only re-shows the prompt next launch, so warn rather than fail the launch.
async fn persist_consent(settings: &crate::database::settings::Settings, value: bool) {
    use crate::database::settings::{
        Setting,
        SettingScope,
    };

    let mut settings = settings.clone();
    if let Err(e) = settings
        .set(Setting::ChatEnableAutoAgentUpgrade, value, Some(SettingScope::Global))
        .await
    {
        tracing::warn!("Failed to save auto-upgrade preference: {e}");
    }
}

#[cfg(test)]
mod tests {
    use agent::agent_config::migration::AgentScope;

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
    fn selection_maps_to_choice() {
        assert_eq!(selection_to_choice(Some(0)), PromptChoice::Enable);
        assert_eq!(selection_to_choice(Some(1)), PromptChoice::Decline);
        // Cancel / I/O error → dismiss (neither enable nor permanently decline).
        assert_eq!(selection_to_choice(None), PromptChoice::Dismiss);
        assert_eq!(selection_to_choice(Some(9)), PromptChoice::Dismiss);
    }
}
