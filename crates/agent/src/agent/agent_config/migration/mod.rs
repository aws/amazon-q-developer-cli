//! Agent-config migration engine: converts V2 (CLI) agent configs to the "universal" format that
//! both the Rust (V2/ACP) and KAS engines load, classifies each config, and rewrites it in place.

pub mod hooks;
pub mod io;
pub mod migrate;
pub mod permissions;
pub mod regex_to_glob;
pub mod scan;
pub mod tool_table;

// Flat re-exports of the engine's public surface so callers import from one path.
pub use io::{
    AgentUpgradeOutcome,
    UpgradeStatus,
    upgrade_agent_file,
};
pub use migrate::AgentClassification;
pub use scan::{
    AgentScope,
    BucketCount,
    ScanCounts,
    ScanDir,
    ScanResult,
    ScannedAgent,
    default_scan_dirs,
    scan_agents,
};
