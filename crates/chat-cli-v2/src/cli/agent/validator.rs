//! Agent validation - checks for reserved/duplicate triggers and other issues.

use std::collections::{
    HashMap,
    HashSet,
};
use std::io::Write;

use crossterm::{
    queue,
    style,
};
use rustyline::KeyEvent;

use super::Agent;
use crate::cli::chat::legacy::parse_shortcut;
use crate::constants::{
    BUILT_IN_AGENTS,
    RESERVED_KEYBOARD_SHORTCUTS,
};
use crate::theme::StyledText;

/// Validate keyboard shortcuts: clear invalid, reserved, and duplicates.
fn validate_keyboard_shortcuts(mut agents: Vec<Agent>, output: &mut impl Write) -> Vec<Agent> {
    let reserved_keys: HashSet<KeyEvent> = RESERVED_KEYBOARD_SHORTCUTS
        .iter()
        .filter_map(|s| parse_shortcut(s).ok())
        .collect();

    let mut to_clear: Vec<usize> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut shortcut_map: HashMap<KeyEvent, Vec<usize>> = HashMap::new();

    // First pass: check invalid/reserved, collect for duplicate detection
    for (idx, agent) in agents.iter().enumerate() {
        if let Some(shortcut_str) = &agent.keyboard_shortcut {
            let is_builtin = BUILT_IN_AGENTS.contains(&agent.name.as_str());

            match parse_shortcut(shortcut_str) {
                Err(_) => {
                    warnings.push(format!(
                        "Agent '{}' has invalid shortcut '{}', shortcut disabled",
                        agent.name, shortcut_str
                    ));
                    to_clear.push(idx);
                },
                Ok(key) if !is_builtin && reserved_keys.contains(&key) => {
                    warnings.push(format!(
                        "Agent '{}' uses reserved shortcut '{}', shortcut disabled",
                        agent.name, shortcut_str
                    ));
                    to_clear.push(idx);
                },
                Ok(key) => shortcut_map.entry(key).or_default().push(idx),
            }
        }
    }

    // Second pass: find duplicates
    for indices in shortcut_map.values().filter(|v| v.len() > 1) {
        let names: Vec<_> = indices.iter().map(|&i| agents[i].name.as_str()).collect();
        warnings.push(format!(
            "Duplicate keyboard shortcut: {}, shortcuts disabled",
            names.join(", ")
        ));
        to_clear.extend(indices);
    }

    // Print warnings and clear
    print_warnings(output, &warnings);
    for idx in to_clear {
        agents[idx].keyboard_shortcut = None;
    }

    agents
}

fn print_warnings(output: &mut impl Write, warnings: &[String]) {
    if warnings.is_empty() {
        return;
    }
    let _ = queue!(
        output,
        StyledText::warning_fg(),
        style::Print("WARNING: "),
        StyledText::reset(),
        style::Print("Agent issues:\n"),
    );
    for w in warnings {
        let _ = queue!(output, style::Print(format!("  - {w}\n")));
    }
}

/// Validate agents: keyboard shortcuts and future validations.
pub fn validate_agents(agents: Vec<Agent>, output: &mut impl Write) -> Vec<Agent> {
    let agents = validate_keyboard_shortcuts(agents, output);
    let _ = output.flush();
    agents
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_agent(name: &str, keyboard_shortcut: Option<&str>) -> Agent {
        Agent {
            name: name.to_string(),
            keyboard_shortcut: keyboard_shortcut.map(String::from),
            ..Default::default()
        }
    }

    #[test]
    fn test_keyboard_valid() {
        let agents = vec![make_agent("a1", Some("ctrl+a"))];
        let result = validate_keyboard_shortcuts(agents, &mut Vec::new());
        assert!(result[0].keyboard_shortcut.is_some());
    }

    #[test]
    fn test_keyboard_invalid() {
        let agents = vec![make_agent("a1", Some("bad+key+combo"))];
        let result = validate_keyboard_shortcuts(agents, &mut Vec::new());
        assert!(result[0].keyboard_shortcut.is_none());
    }

    #[test]
    fn test_keyboard_reserved() {
        let agents = vec![make_agent("a1", Some("ctrl+c"))];
        let result = validate_keyboard_shortcuts(agents, &mut Vec::new());
        assert!(result[0].keyboard_shortcut.is_none());
    }

    #[test]
    fn test_keyboard_duplicates() {
        let agents = vec![make_agent("a1", Some("ctrl+a")), make_agent("a2", Some("ctrl+a"))];
        let result = validate_keyboard_shortcuts(agents, &mut Vec::new());
        assert!(result[0].keyboard_shortcut.is_none());
        assert!(result[1].keyboard_shortcut.is_none());
    }

    #[test]
    fn test_builtin_can_use_reserved() {
        let agents = vec![
            Agent {
                name: "kiro_planner".to_string(),
                keyboard_shortcut: Some("shift+tab".to_string()),
                ..Default::default()
            },
            make_agent("custom", Some("shift+tab")),
        ];
        let result = validate_keyboard_shortcuts(agents, &mut Vec::new());
        // Built-in agent keeps the shortcut
        assert_eq!(result[0].keyboard_shortcut, Some("shift+tab".to_string()));
        // Custom agent loses it
        assert!(result[1].keyboard_shortcut.is_none());
    }
}
