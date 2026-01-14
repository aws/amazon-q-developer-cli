use std::io::Write;
use std::path::Path;
use std::process::{
    Command,
    ExitStatus,
    Stdio,
};

use crate::cli::chat::ChatError;
use crate::util::env_var::try_get_diff_tool;

#[derive(Clone, Copy)]
enum DiffStrategy {
    /// Pipe unified diff to stdin (delta, diff-so-fancy, colordiff, bat, ydiff)
    UnifiedStdin,
    /// Pass two file paths as arguments (difft, meld, vimdiff)
    TwoFile,
    /// Pass two file paths with --diff flag (code, codium)
    TwoFileWithDiffFlag,
    /// icdiff with special -L and -N flags
    Icdiff,
    /// Unknown tool: try unified stdin first, fallback to two-file
    AutoDetect,
}

/// Configuration for a diff tool
struct ToolConfig {
    strategy: DiffStrategy,
    /// GUI tools read files async, so we shouldn't cleanup temp files
    is_async: bool,
    /// Tool prints output to terminal
    is_inline: bool,
}

impl ToolConfig {
    const fn new(strategy: DiffStrategy, is_async: bool, is_inline: bool) -> Self {
        Self {
            strategy,
            is_async,
            is_inline,
        }
    }
}

fn get_tool_config(name: &str) -> ToolConfig {
    match name {
        // Unified diff stdin tools (inline, sync)
        "delta" | "diff-so-fancy" | "colordiff" | "diff-highlight" | "bat" | "ydiff" => {
            ToolConfig::new(DiffStrategy::UnifiedStdin, false, true)
        },
        // VS Code family (async GUI, needs --diff flag)
        "code" | "codium" | "vscode" => ToolConfig::new(DiffStrategy::TwoFileWithDiffFlag, true, false),
        // difftastic (inline, sync)
        "difft" => ToolConfig::new(DiffStrategy::TwoFile, false, true),
        // icdiff (inline, sync, special flags)
        "icdiff" => ToolConfig::new(DiffStrategy::Icdiff, false, true),
        // GUI tools (async, two file)
        "meld" | "kdiff3" | "opendiff" => ToolConfig::new(DiffStrategy::TwoFile, true, false),
        // Terminal-based editors (sync, two file)
        "vimdiff" | "nvim" | "vim" => ToolConfig::new(DiffStrategy::TwoFile, false, false),
        // Unknown: try auto-detect (unified stdin -> two-file fallback)
        _ => ToolConfig::new(DiffStrategy::AutoDetect, false, true),
    }
}

pub fn has_diff_tool() -> bool {
    try_get_diff_tool().is_ok()
}

pub fn is_inline_diff_tool() -> bool {
    get_tool_name().is_some_and(|name| get_tool_config(&name).is_inline)
}

pub fn diff_with_tool(
    before_content: &str,
    after_content: &str,
    label: &str,
    start_line: usize,
) -> Result<(), ChatError> {
    let cmd = try_get_diff_tool().map_err(|_ignored| ChatError::Custom("KIRO_DIFF_TOOL not configured".into()))?;
    let tool_name = get_tool_name().unwrap_or_default();
    let config = get_tool_config(&tool_name);

    // Create temp files
    let temp_dir = std::env::temp_dir();
    let safe_label = label.replace(['/', '\\', ':'], "_");
    let before_file = temp_dir.join(format!("kiro_before_{safe_label}"));
    let after_file = temp_dir.join(format!("kiro_after_{safe_label}"));

    let before_content = ensure_trailing_newline(before_content);
    let after_content = ensure_trailing_newline(after_content);

    std::fs::write(&before_file, &before_content)
        .map_err(|e| ChatError::Custom(format!("Failed to create temp file: {e}").into()))?;
    std::fs::write(&after_file, &after_content)
        .map_err(|e| ChatError::Custom(format!("Failed to create temp file: {e}").into()))?;

    let ctx = DiffContext {
        cmd: &cmd,
        config: &config,
        before_path: &before_file,
        after_path: &after_file,
        label,
        before_content: &before_content,
        after_content: &after_content,
        start_line,
    };
    let result = launch_tool(&ctx);

    // Async tools (GUI) read files after command returns, don't cleanup
    if !config.is_async {
        let _ = std::fs::remove_file(&before_file);
        let _ = std::fs::remove_file(&after_file);
    }

    result
}

fn get_tool_name() -> Option<String> {
    let cmd = try_get_diff_tool().ok()?;
    let path = cmd.split_whitespace().next()?;
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    Some(name.to_string())
}

fn ensure_trailing_newline(content: &str) -> String {
    if content.is_empty() || content.ends_with('\n') {
        content.to_string()
    } else {
        format!("{content}\n")
    }
}

/// Check diff tool exit status. For diff tools: 0 = no diff, 1 = has diff
fn check_diff_status(status: ExitStatus, tool_name: &str) -> Result<(), ChatError> {
    match status.code() {
        Some(0 | 1) => Ok(()),
        Some(code) => Err(ChatError::Custom(
            format!("Diff tool '{tool_name}' exited with error code: {code}").into(),
        )),
        None => Err(ChatError::Custom(
            format!("Diff tool '{tool_name}' terminated by signal").into(),
        )),
    }
}

fn spawn_error(tool_name: &str, e: std::io::Error) -> ChatError {
    if e.kind() == std::io::ErrorKind::NotFound {
        ChatError::Custom(
            format!("Couldn't find the diff tool '{tool_name}'. Make sure it's installed and available in your PATH.")
                .into(),
        )
    } else {
        ChatError::Custom(format!("Failed to launch '{tool_name}': {e}").into())
    }
}

fn parse_command(cmd: &str) -> Result<(String, Vec<String>), ChatError> {
    let parts = shlex::split(cmd).ok_or_else(|| ChatError::Custom("Invalid diff tool command".into()))?;
    let mut iter = parts.into_iter();
    let tool = iter
        .next()
        .ok_or_else(|| ChatError::Custom("Empty diff tool command".into()))?;
    Ok((tool, iter.collect()))
}

struct DiffContext<'a> {
    cmd: &'a str,
    config: &'a ToolConfig,
    before_path: &'a Path,
    after_path: &'a Path,
    label: &'a str,
    before_content: &'a str,
    after_content: &'a str,
    start_line: usize,
}

fn launch_tool(ctx: &DiffContext<'_>) -> Result<(), ChatError> {
    let (tool, args) = parse_command(ctx.cmd)?;

    match ctx.config.strategy {
        DiffStrategy::UnifiedStdin => {
            let unified = generate_unified_diff(ctx.before_content, ctx.after_content, ctx.label, ctx.start_line);
            launch_with_stdin(&tool, &args, &unified)
        },
        DiffStrategy::TwoFile => launch_two_file(&tool, &args, ctx.before_path, ctx.after_path),
        DiffStrategy::TwoFileWithDiffFlag => launch_with_diff_flag(&tool, &args, ctx.before_path, ctx.after_path),
        DiffStrategy::Icdiff => launch_icdiff(&tool, &args, ctx.before_path, ctx.after_path, ctx.label),
        DiffStrategy::AutoDetect => {
            // Try unified stdin first, fallback to two-file if it fails
            let unified = generate_unified_diff(ctx.before_content, ctx.after_content, ctx.label, ctx.start_line);
            launch_with_stdin(&tool, &args, &unified)
                .or_else(|_| launch_two_file(&tool, &args, ctx.before_path, ctx.after_path))
        },
    }
}

fn launch_with_stdin(tool: &str, args: &[String], unified_diff: &str) -> Result<(), ChatError> {
    let mut child = Command::new(tool)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| spawn_error(tool, e))?;

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(unified_diff.as_bytes());
    }

    let status = child
        .wait()
        .map_err(|e| ChatError::Custom(format!("Failed to wait for diff tool: {e}").into()))?;
    check_diff_status(status, tool)
}

fn launch_two_file(tool: &str, args: &[String], before: &Path, after: &Path) -> Result<(), ChatError> {
    let status = Command::new(tool)
        .args(args)
        .arg(before)
        .arg(after)
        .status()
        .map_err(|e| spawn_error(tool, e))?;
    check_diff_status(status, tool)
}

fn launch_with_diff_flag(tool: &str, args: &[String], before: &Path, after: &Path) -> Result<(), ChatError> {
    let has_diff_flag = args.iter().any(|a| a == "--diff" || a == "-d");

    let mut cmd = Command::new(tool);
    cmd.args(args);
    if !has_diff_flag {
        cmd.arg("--diff");
    }
    cmd.arg(before).arg(after);

    let status = cmd.status().map_err(|e| spawn_error(tool, e))?;
    check_diff_status(status, tool)
}

fn launch_icdiff(tool: &str, args: &[String], before: &Path, after: &Path, label: &str) -> Result<(), ChatError> {
    let status = Command::new(tool)
        .arg("-L")
        .arg(format!("a/{label}"))
        .arg("-L")
        .arg(format!("b/{label}"))
        .arg("-N")
        .args(args)
        .arg(before)
        .arg(after)
        .status()
        .map_err(|e| spawn_error(tool, e))?;
    check_diff_status(status, tool)
}

fn generate_unified_diff(before: &str, after: &str, label: &str, start_line: usize) -> String {
    let before_lines = if before.is_empty() { 0 } else { before.lines().count() };
    let after_lines = if after.is_empty() { 0 } else { after.lines().count() };

    let before_start = if before_lines == 0 { 0 } else { start_line };
    let after_start = if after_lines == 0 { 0 } else { start_line };

    let mut output =
        format!("--- a/{label}\n+++ b/{label}\n@@ -{before_start},{before_lines} +{after_start},{after_lines} @@\n");

    let diff = similar::TextDiff::from_lines(before, after);
    for change in diff.iter_all_changes() {
        let sign = match change.tag() {
            similar::ChangeTag::Delete => "-",
            similar::ChangeTag::Insert => "+",
            similar::ChangeTag::Equal => " ",
        };
        let line = change.to_string_lossy();
        output.push_str(sign);
        output.push_str(&line);
        if !line.ends_with('\n') {
            output.push('\n');
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_config() {
        // Unified stdin tools
        assert!(matches!(get_tool_config("delta").strategy, DiffStrategy::UnifiedStdin));
        assert!(get_tool_config("delta").is_inline);
        assert!(!get_tool_config("delta").is_async);

        // VS Code
        assert!(matches!(
            get_tool_config("code").strategy,
            DiffStrategy::TwoFileWithDiffFlag
        ));
        assert!(!get_tool_config("code").is_inline);
        assert!(get_tool_config("code").is_async);

        // difft
        assert!(matches!(get_tool_config("difft").strategy, DiffStrategy::TwoFile));
        assert!(get_tool_config("difft").is_inline);

        // icdiff
        assert!(matches!(get_tool_config("icdiff").strategy, DiffStrategy::Icdiff));

        // GUI tools
        assert!(get_tool_config("meld").is_async);
        assert!(!get_tool_config("vimdiff").is_async);

        // Unknown tools use AutoDetect
        assert!(matches!(
            get_tool_config("unknown-tool").strategy,
            DiffStrategy::AutoDetect
        ));
        assert!(matches!(
            get_tool_config("beyond-compare").strategy,
            DiffStrategy::AutoDetect
        ));
    }

    #[test]
    fn test_ensure_trailing_newline() {
        assert_eq!(ensure_trailing_newline(""), "");
        assert_eq!(ensure_trailing_newline("hello"), "hello\n");
        assert_eq!(ensure_trailing_newline("hello\n"), "hello\n");
    }

    #[test]
    fn test_generate_unified_diff() {
        let diff = generate_unified_diff("old\n", "new\n", "test.txt", 10);
        assert!(diff.contains("--- a/test.txt"));
        assert!(diff.contains("+++ b/test.txt"));
        assert!(diff.contains("@@ -10,1 +10,1 @@"));
    }

    #[test]
    fn test_parse_command() {
        let (tool, args) = parse_command("delta --side-by-side").unwrap();
        assert_eq!(tool, "delta");
        assert_eq!(args, vec!["--side-by-side"]);

        let (tool, args) = parse_command("code").unwrap();
        assert_eq!(tool, "code");
        assert!(args.is_empty());
    }
}
