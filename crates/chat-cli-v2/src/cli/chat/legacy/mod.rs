//! Legacy code preserved for external callers.
//!
//! This module contains types and functions from the old CLI that are still
//! needed by the ACP agent, MCP client, and other modules.
//!
//! TODO: Move these to more appropriate locations in the codebase.

pub mod custom_tool;
pub mod model;
pub mod tools;
pub mod util;

use std::path::PathBuf;

use rustyline::{
    KeyCode,
    KeyEvent,
    Modifiers,
};

use crate::os::Os;
use crate::util::paths::PathResolver;

/// Get workspace MCP config path
pub fn workspace_mcp_config_path(os: &Os) -> eyre::Result<PathBuf> {
    Ok(PathResolver::new(os).workspace().mcp_config()?)
}

/// Get global MCP config path
pub fn global_mcp_config_path(os: &Os) -> eyre::Result<PathBuf> {
    Ok(PathResolver::new(os).global().mcp_config()?)
}

// Re-export is_native_tool from tools module
pub use tools::is_native_tool;

/// Performs tilde expansion and other required sanitization for tool path arguments.
pub fn sanitize_path_tool_arg(os: &Os, path: impl AsRef<std::path::Path>) -> PathBuf {
    let mut res = PathBuf::new();
    let mut path = path.as_ref().components();
    match path.next() {
        Some(p) if p.as_os_str() == "~" => {
            res.push(os.env.home().unwrap_or_default());
        },
        Some(p) => res.push(p),
        None => return res,
    }
    for p in path {
        res.push(p);
    }
    os.fs.chroot_path(res)
}

/// Parse a shortcut string like "ctrl+shift+a" or "shift+tab" into a KeyEvent
pub fn parse_shortcut(shortcut: &str) -> Result<KeyEvent, String> {
    if shortcut.is_empty() {
        return Err("Empty shortcut".to_string());
    }

    let mut modifiers = Modifiers::empty();
    let mut key_part: Option<String> = None;

    for part in shortcut.split('+') {
        let part_lower = part.trim().to_lowercase();
        match part_lower.as_str() {
            "ctrl" => {
                if modifiers.contains(Modifiers::CTRL) {
                    return Err("Duplicate 'ctrl' modifier".to_string());
                }
                modifiers.insert(Modifiers::CTRL);
            },
            "shift" => {
                if modifiers.contains(Modifiers::SHIFT) {
                    return Err("Duplicate 'shift' modifier".to_string());
                }
                modifiers.insert(Modifiers::SHIFT);
            },
            "alt" => {
                if modifiers.contains(Modifiers::ALT) {
                    return Err("Duplicate 'alt' modifier".to_string());
                }
                modifiers.insert(Modifiers::ALT);
            },
            key if !key.is_empty() => {
                if let Some(existing) = &key_part {
                    return Err(format!("Multiple keys specified: '{}' and '{}'", existing, key));
                }
                key_part = Some(part_lower);
            },
            _ => {},
        }
    }

    let key_str = key_part.ok_or_else(|| "No key specified".to_string())?;

    // Special case: shift+tab maps to BackTab
    if key_str == "tab" && modifiers.contains(Modifiers::SHIFT) {
        modifiers.remove(Modifiers::SHIFT);
        return Ok(KeyEvent(KeyCode::BackTab, modifiers));
    }

    let key_code = if key_str == "tab" {
        KeyCode::Tab
    } else if key_str.len() == 1 {
        let ch = key_str.chars().next().unwrap();
        if ch.is_ascii_alphabetic() {
            let ch = if modifiers.contains(Modifiers::SHIFT) {
                ch.to_ascii_uppercase()
            } else {
                ch
            };
            KeyCode::Char(ch)
        } else if ch.is_ascii_digit() {
            KeyCode::Char(ch)
        } else {
            return Err(format!("Invalid key: '{key_str}'"));
        }
    } else if key_str.starts_with('f') && key_str.len() >= 2 {
        let num_str = &key_str[1..];
        match num_str.parse::<u8>() {
            Ok(n) if (1..=12).contains(&n) => KeyCode::F(n),
            _ => return Err(format!("Invalid function key: '{key_str}'")),
        }
    } else {
        return Err(format!("Invalid key: '{key_str}'"));
    };

    Ok(KeyEvent(key_code, modifiers))
}
