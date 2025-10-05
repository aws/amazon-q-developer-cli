use std::path::Path;
use std::process::Command;

/// Launch the user's preferred editor with the given file path.
///
/// This function properly parses the EDITOR environment variable to handle
/// editors that require arguments (e.g., "emacsclient -nw").
///
/// # Arguments
/// * `file_path` - Path to the file to open in the editor
///
/// # Returns
/// * `Ok(())` if the editor was launched successfully and exited with success
/// * `Err` if the editor failed to launch or exited with an error
pub fn launch_editor(file_path: &Path) -> eyre::Result<()> {
    let editor_cmd = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());

    // Parse the editor command to handle arguments
    let mut parts = shlex::split(&editor_cmd)
        .ok_or_else(|| eyre::eyre!("Failed to parse EDITOR command"))?;

    if parts.is_empty() {
        eyre::bail!("EDITOR environment variable is empty");
    }

    let editor_bin = parts.remove(0);

    let mut cmd = Command::new(editor_bin);
    for arg in parts {
        cmd.arg(arg);
    }

    let status = cmd.arg(file_path).status()?;

    if !status.success() {
        eyre::bail!("Editor process did not exit with success");
    }

    Ok(())
}
