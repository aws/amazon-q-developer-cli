//! Read-only classification scan: walks the workspace-local and user-global agent dirs and
//! classifies every agent from its on-disk content.

use std::path::PathBuf;

use serde::{
    Deserialize,
    Serialize,
};
use typeshare::typeshare;

use super::io::{
    agent_name_from_path,
    is_externally_managed,
    list_agent_files,
    looks_like_agent,
    parse_agent_file,
};
use super::migrate::{
    AgentClassification,
    upgrade_agent_config,
};
use super::permissions::MigrationWarning;
use crate::agent::agent_config::load::{
    configured_agent_dir,
    resolve_global_agents_dir,
    resolve_workspace_agents_dir,
};
use crate::agent::util::providers::SystemProvider;

/// Where a scanned agent lives.
#[typeshare]
// clap::ValueEnum defaults to kebab-case; Local/Global are single words, matching serde lowercase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum AgentScope {
    Local,
    Global,
}

/// One classified agent found during a scan.
#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScannedAgent {
    pub name: String,
    pub scope: AgentScope,
    pub classification: AgentClassification,
    /// Absolute path of the source `.json` file.
    pub source_path: String,
    /// Conversion warnings from the V3 derivation (empty for v3-only / no-trust agents).
    pub warnings: Vec<MigrationWarning>,
}

/// Per-scope counts for one classification bucket.
#[typeshare]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BucketCount {
    pub local: u32,
    pub global: u32,
    pub total: u32,
}

/// Per-classification bucket counts. An explicit struct (not a map) so the wire shape matches the
/// fixed object the TUI indexes.
#[typeshare]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanCounts {
    #[serde(rename = "v2-only")]
    pub v2_only: BucketCount,
    #[serde(rename = "universal-out-of-sync")]
    pub universal_out_of_sync: BucketCount,
    #[serde(rename = "universal-in-sync")]
    pub universal_in_sync: BucketCount,
    #[serde(rename = "v3-only")]
    pub v3_only: BucketCount,
}

impl ScanCounts {
    /// The bucket for a classification, mutable so the tally can increment it.
    fn bucket_mut(&mut self, classification: AgentClassification) -> &mut BucketCount {
        match classification {
            AgentClassification::V2Only => &mut self.v2_only,
            AgentClassification::UniversalOutOfSync => &mut self.universal_out_of_sync,
            AgentClassification::UniversalInSync => &mut self.universal_in_sync,
            AgentClassification::V3Only => &mut self.v3_only,
        }
    }
}

/// Result of scanning one or more agent dirs.
#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub agents: Vec<ScannedAgent>,
    pub counts: ScanCounts,
    /// Total number of distinct agents found.
    pub total: u32,
}

/// A directory to scan, tagged with its scope.
#[derive(Debug, Clone)]
pub struct ScanDir {
    pub dir: PathBuf,
    pub scope: AgentScope,
}

/// Default scan dirs: workspace-local first, then user-global. Reuses the loader's resolvers so the
/// configured agent directory, test/home overrides, and `.amazonq` fallbacks stay honored; falls
/// back to the canonical `.kiro/agents` path when a dir doesn't yet exist so the scope is still
/// represented (its `list_agent_files` yields an empty list).
pub fn default_scan_dirs(system: &dyn SystemProvider) -> Vec<ScanDir> {
    let local = if configured_agent_dir(system).is_some() {
        None
    } else {
        resolve_workspace_agents_dir(system).or_else(|| system.cwd().ok().map(|cwd| cwd.join(".kiro").join("agents")))
    };
    let global = resolve_global_agents_dir(system).or_else(|| {
        system
            .home()
            .map(|home| crate::agent::util::directories::kiro_home_dir_in(&home).join("agents"))
    });

    let mut dirs = Vec::new();
    if let Some(dir) = local {
        dirs.push(ScanDir {
            dir,
            scope: AgentScope::Local,
        });
    }
    if let Some(dir) = global {
        dirs.push(ScanDir {
            dir,
            scope: AgentScope::Global,
        });
    }
    dirs
}

/// Scan agent directories and classify every agent. Top-level files only; backup files are filtered
/// out by `list_agent_files`.
pub fn scan_agents(dirs: &[ScanDir]) -> ScanResult {
    let mut agents: Vec<ScannedAgent> = Vec::new();

    for ScanDir { dir, scope } in dirs {
        for path in list_agent_files(dir) {
            let Some(config) = parse_agent_file(&path) else {
                continue;
            };
            if !looks_like_agent(&config) || is_externally_managed(&config) {
                continue;
            }

            let result = upgrade_agent_config(&config);
            agents.push(ScannedAgent {
                name: agent_name_from_path(&path),
                scope: *scope,
                classification: result.classification,
                source_path: path.to_string_lossy().into_owned(),
                warnings: result.warnings,
            });
        }
    }

    let mut counts = ScanCounts::default();
    for agent in &agents {
        let bucket = counts.bucket_mut(agent.classification);
        match agent.scope {
            AgentScope::Local => bucket.local += 1,
            AgentScope::Global => bucket.global += 1,
        }
        bucket.total += 1;
    }

    let total = agents.len() as u32;
    ScanResult { agents, counts, total }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;
    use tempfile::TempDir;

    use super::*;

    /// A row for order-independent assertions.
    #[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
    struct Row {
        name: String,
        scope: String,
        classification: String,
    }

    /// Two temp dirs (local + global) that stay alive for the length of a scan.
    struct Scanner {
        _local: TempDir,
        _global: TempDir,
        dirs: Vec<ScanDir>,
    }

    impl Scanner {
        fn new() -> Self {
            let local = tempfile::tempdir().expect("tempdir");
            let global = tempfile::tempdir().expect("tempdir");
            let dirs = vec![
                ScanDir {
                    dir: local.path().to_path_buf(),
                    scope: AgentScope::Local,
                },
                ScanDir {
                    dir: global.path().to_path_buf(),
                    scope: AgentScope::Global,
                },
            ];
            Self {
                _local: local,
                _global: global,
                dirs,
            }
        }

        fn write(&self, scope: AgentScope, files: &[(&str, Value)]) {
            let dir = match scope {
                AgentScope::Local => self._local.path(),
                AgentScope::Global => self._global.path(),
            };
            for (name, content) in files {
                let serialized = serde_json::to_string_pretty(content).expect("serializes");
                std::fs::write(dir.join(name), serialized).expect("write");
            }
        }

        fn scan(&self) -> ScanResult {
            scan_agents(&self.dirs)
        }

        /// Sorted (name, scope, classification) rows.
        fn rows(&self) -> Vec<Row> {
            let mut rows: Vec<Row> = self
                .scan()
                .agents
                .into_iter()
                .map(|a| Row {
                    name: a.name,
                    scope: serde_json::to_value(a.scope).unwrap().as_str().unwrap().to_string(),
                    classification: serde_json::to_value(a.classification)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_string(),
                })
                .collect();
            rows.sort();
            rows
        }
    }

    fn row(name: &str, scope: &str, classification: &str) -> Row {
        Row {
            name: name.to_string(),
            scope: scope.to_string(),
            classification: classification.to_string(),
        }
    }

    #[test]
    fn v2_only_marker_tools() {
        let s = Scanner::new();
        s.write(AgentScope::Local, &[(
            "a.json",
            serde_json::json!({ "tools": ["fs_read"] }),
        )]);
        assert_eq!(s.rows(), vec![row("a", "local", "v2-only")]);
    }

    #[test]
    fn v2_only_allowed_tools_triggers_trust() {
        let s = Scanner::new();
        s.write(AgentScope::Local, &[(
            "b.json",
            serde_json::json!({ "tools": ["read"], "allowedTools": ["fs_read"] }),
        )]);
        assert_eq!(s.rows(), vec![row("b", "local", "v2-only")]);
    }

    #[test]
    fn v2_only_tools_settings_triggers_trust() {
        let s = Scanner::new();
        s.write(AgentScope::Local, &[(
            "c.json",
            serde_json::json!({
                "tools": ["execute_bash"],
                "toolsSettings": { "execute_bash": { "allowedCommands": ["git status"] } }
            }),
        )]);
        assert_eq!(s.rows(), vec![row("c", "local", "v2-only")]);
    }

    #[test]
    fn universal_out_of_sync() {
        let s = Scanner::new();
        s.write(AgentScope::Local, &[(
            "o.json",
            serde_json::json!({ "tools": ["fs_read", "fs_write"], "permissions": { "rules": [] } }),
        )]);
        assert_eq!(s.rows(), vec![row("o", "local", "universal-out-of-sync")]);
    }

    #[test]
    fn universal_in_sync() {
        let s = Scanner::new();
        s.write(AgentScope::Local, &[(
            "u.json",
            serde_json::json!({
                "tools": ["shell"],
                "toolsSettings": {
                    "execute_bash": {
                        "allowedCommands": ["git status", "ls"],
                        "deniedCommands": ["rm -rf"]
                    }
                },
                "permissions": {
                    "rules": [
                        { "capability": "shell", "match": ["git status", "ls"], "effect": "allow" },
                        { "capability": "shell", "match": ["rm -rf"], "effect": "deny" }
                    ]
                }
            }),
        )]);
        assert_eq!(s.rows(), vec![row("u", "local", "universal-in-sync")]);
    }

    #[test]
    fn v3_only_trust_field_no_v2_trust() {
        let s = Scanner::new();
        s.write(AgentScope::Local, &[(
            "k.json",
            serde_json::json!({ "tools": ["read"], "permissions": { "rules": [] } }),
        )]);
        assert_eq!(s.rows(), vec![row("k", "local", "v3-only")]);
    }

    #[test]
    fn v3_only_include_powers_only() {
        let s = Scanner::new();
        s.write(AgentScope::Local, &[(
            "p.json",
            serde_json::json!({ "prompt": "you help with X", "includePowers": ["foo"] }),
        )]);
        assert_eq!(s.rows(), vec![row("p", "local", "v3-only")]);
    }

    #[test]
    fn agents_classified_per_scope() {
        let s = Scanner::new();
        s.write(AgentScope::Local, &[(
            "a.json",
            serde_json::json!({ "tools": ["fs_read"] }),
        )]);
        s.write(AgentScope::Global, &[(
            "b.json",
            serde_json::json!({ "tools": ["fs_write"] }),
        )]);
        assert_eq!(s.rows(), vec![
            row("a", "local", "v2-only"),
            row("b", "global", "v2-only")
        ]);
    }

    #[test]
    fn non_agent_json_ignored() {
        let s = Scanner::new();
        s.write(AgentScope::Local, &[
            ("mcp.json", serde_json::json!({ "mcpServers": {} })),
            ("a.json", serde_json::json!({ "tools": ["fs_read"] })),
        ]);
        assert_eq!(s.rows(), vec![row("a", "local", "v2-only")]);
    }

    #[test]
    fn backup_files_excluded() {
        let s = Scanner::new();
        s.write(AgentScope::Local, &[
            ("a.json", serde_json::json!({ "tools": ["fs_read"] })),
            ("a.json.bak", serde_json::json!({ "tools": ["fs_read"] })),
            ("a.json.bak.1", serde_json::json!({ "tools": ["fs_read"] })),
        ]);
        assert_eq!(s.rows(), vec![row("a", "local", "v2-only")]);
    }

    #[test]
    fn aim_managed_agents_excluded() {
        let s = Scanner::new();
        s.write(AgentScope::Local, &[
            (
                "managed.json",
                serde_json::json!({ "description": "A workspace agent managed by AIM", "tools": ["fs_read"] }),
            ),
            ("mine.json", serde_json::json!({ "tools": ["fs_read"] })),
        ]);
        assert_eq!(s.rows(), vec![row("mine", "local", "v2-only")]);
    }

    #[test]
    fn tallies_by_scope_and_classification_exposes_warnings() {
        let s = Scanner::new();
        // V2-only with a lossy regex (char class -> regex-shell-pattern warning).
        s.write(AgentScope::Local, &[(
            "lossy.json",
            serde_json::json!({
                "name": "lossy",
                "tools": ["execute_bash"],
                "toolsSettings": { "execute_bash": { "allowedCommands": ["sleep [0-9]+"] } }
            }),
        )]);
        // V2-only autoAllowReadonly (clean; no warning) and a V3-only agent (V2 skips).
        s.write(AgentScope::Global, &[
            (
                "readonly.json",
                serde_json::json!({
                    "name": "readonly",
                    "tools": ["execute_bash"],
                    "toolsSettings": { "execute_bash": { "autoAllowReadonly": true } }
                }),
            ),
            (
                "kas-only.json",
                serde_json::json!({ "name": "kas-only", "tools": ["read"], "permissions": { "rules": [] } }),
            ),
        ]);

        let scan = s.scan();
        assert_eq!(scan.total, 3);
        assert_eq!(scan.counts.v2_only.total, 2);
        assert_eq!(scan.counts.v2_only.local, 1);
        assert_eq!(scan.counts.v2_only.global, 1);
        assert_eq!(scan.counts.v3_only.total, 1);

        let mut with_warnings: Vec<String> = scan
            .agents
            .iter()
            .filter(|a| !a.warnings.is_empty())
            .map(|a| a.name.clone())
            .collect();
        with_warnings.sort();
        assert_eq!(with_warnings, vec!["lossy".to_string()]);

        let lossy = scan.agents.iter().find(|a| a.name == "lossy").expect("lossy present");
        let kinds: Vec<String> = lossy
            .warnings
            .iter()
            .map(|w| serde_json::to_value(w.kind).unwrap().as_str().unwrap().to_string())
            .collect();
        assert_eq!(kinds, vec!["regex-shell-pattern".to_string()]);
    }

    #[test]
    fn empty_workspace_zero_counts() {
        let s = Scanner::new();
        let scan = s.scan();
        assert_eq!(scan.total, 0);
        assert_eq!(scan.counts.v2_only.total, 0);
        assert_eq!(scan.counts.universal_in_sync.total, 0);
        assert!(scan.agents.is_empty());
    }

    #[test]
    fn configured_agent_dir_excludes_workspace_scan() {
        use crate::agent::util::test::TestProvider;

        let root = tempfile::tempdir().unwrap();
        let workspace = root.path().join(".kiro/agents");
        let configured = root.path().join("packaged-agents");
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::create_dir_all(&configured).unwrap();
        std::fs::write(workspace.join("shadow.json"), r#"{"tools":["read"]}"#).unwrap();
        std::fs::write(configured.join("kiro-help.json"), r#"{"tools":["read"]}"#).unwrap();
        let provider = TestProvider::new_with_base(root.path())
            .with_cwd(root.path())
            .with_var("KIRO_AGENT_CONFIG_DIR", configured.to_string_lossy());

        let dirs = default_scan_dirs(&provider);

        assert_eq!(dirs.len(), 1);
        assert_eq!(dirs[0].scope, AgentScope::Global);
        assert_eq!(dirs[0].dir, configured);
        let result = scan_agents(&dirs);
        assert_eq!(result.total, 1);
        assert_eq!(result.agents[0].name, "kiro-help");
    }
}
