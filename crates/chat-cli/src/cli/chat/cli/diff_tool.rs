use std::path::Path;
use uuid::Uuid;

use crate::cli::chat::ChatError;
use crate::util::env_var::try_get_diff_tool;

/// Check if custom diff tool is configured
pub fn has_diff_tool() -> bool {
    try_get_diff_tool().is_ok()
}

/// Launch custom diff tool with two file paths
pub fn launch_diff_tool(before_path: &Path, after_path: &Path) -> Result<(), ChatError> {
    let diff_tool_cmd = try_get_diff_tool()
        .map_err(|_| ChatError::Custom("Q_DIFF_TOOL not configured".into()))?;

    let mut parts = shlex::split(&diff_tool_cmd)
        .ok_or_else(|| ChatError::Custom("Failed to parse Q_DIFF_TOOL command".into()))?;

    if parts.is_empty() {
        return Err(ChatError::Custom("Q_DIFF_TOOL is empty".into()));
    }

    let tool_bin = parts.remove(0);
    let mut cmd = std::process::Command::new(tool_bin);

    for arg in parts {
        cmd.arg(arg);
    }

    cmd
        .arg(before_path)
        .arg(after_path)
        .status()
        .map_err(|e| ChatError::Custom(format!("Failed to launch diff tool: {}", e).into()))?;

    Ok(())
}

/// Create temporary files and launch diff tool with content
pub fn diff_with_tool(before_content: &str, after_content: &str, label: &str) -> Result<(), ChatError> {
    let temp_dir = std::env::temp_dir();
    let uuid = Uuid::new_v4();
    
    // Sanitize label to create valid filename
    let safe_label = label.replace(['/', '\\', ':'], "_");
    
    let before_file = temp_dir.join(format!("q_diff_before_{}_{}.txt", safe_label, uuid));
    let after_file = temp_dir.join(format!("q_diff_after_{}_{}.txt", safe_label, uuid));

    std::fs::write(&before_file, before_content)
        .map_err(|e| ChatError::Custom(format!("Failed to create before file: {}", e).into()))?;

    std::fs::write(&after_file, after_content)
        .map_err(|e| ChatError::Custom(format!("Failed to create after file: {}", e).into()))?;

    // Don't cleanup - let OS temp directory cleanup handle it
    // Files need to persist for async tools and approval flow
    launch_diff_tool(&before_file, &after_file)
}
