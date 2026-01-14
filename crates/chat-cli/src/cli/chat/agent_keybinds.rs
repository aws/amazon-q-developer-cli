use eyre::Result;
use rustyline::history::FileHistory;
use rustyline::{
    Cmd,
    Editor,
    EventHandler,
    KeyCode,
    KeyEvent,
    Modifiers,
};

use super::agent_swap::AgentSwapState;

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

/// Generic handler for agent keyboard shortcuts
pub struct AgentSwapHandler {
    agent_name: String,
    welcome_message: Option<String>,
    swap_state: AgentSwapState,
}

impl AgentSwapHandler {
    pub fn new(agent_name: String, welcome_message: Option<String>, swap_state: AgentSwapState) -> Self {
        Self {
            agent_name,
            welcome_message,
            swap_state,
        }
    }
}

impl rustyline::ConditionalEventHandler for AgentSwapHandler {
    fn handle(
        &self,
        _evt: &rustyline::Event,
        _n: rustyline::RepeatCount,
        _positive: bool,
        _ctx: &rustyline::EventContext<'_>,
    ) -> Option<Cmd> {
        self.swap_state
            .trigger_swap(&self.agent_name, self.welcome_message.clone(), None);
        Some(Cmd::AcceptLine)
    }
}

/// Binds agent-related keyboard shortcuts
pub fn bind_agent_shortcuts<H: rustyline::Helper>(
    rl: &mut Editor<H, FileHistory>,
    agents: &crate::cli::agent::Agents,
    swap_state: &AgentSwapState,
) -> Result<()> {
    swap_state.set_current_agent(agents.active_idx.clone());

    for (agent_name, agent) in &agents.agents {
        if let Some(shortcut_str) = &agent.keyboard_shortcut
            && let Ok(key_event) = parse_shortcut(shortcut_str)
        {
            rl.bind_sequence(
                key_event,
                EventHandler::Conditional(Box::new(AgentSwapHandler::new(
                    agent_name.clone(),
                    agent.welcome_message.clone(),
                    swap_state.clone(),
                ))),
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ctrl_a() {
        let result = parse_shortcut("ctrl+a").unwrap();
        assert_eq!(result.0, KeyCode::Char('a'));
        assert!(result.1.contains(Modifiers::CTRL));
    }

    #[test]
    fn test_ctrl_shift_a() {
        let result = parse_shortcut("ctrl+shift+a").unwrap();
        assert_eq!(result.0, KeyCode::Char('A'));
        assert!(result.1.contains(Modifiers::CTRL));
        assert!(result.1.contains(Modifiers::SHIFT));
    }

    #[test]
    fn test_shift_tab_maps_to_backtab() {
        let result = parse_shortcut("shift+tab").unwrap();
        assert_eq!(result.0, KeyCode::BackTab);
        assert!(!result.1.contains(Modifiers::SHIFT));
    }

    #[test]
    fn test_function_key() {
        let result = parse_shortcut("alt+f1").unwrap();
        assert_eq!(result.0, KeyCode::F(1));
        assert!(result.1.contains(Modifiers::ALT));
    }

    #[test]
    fn test_case_insensitive() {
        let result = parse_shortcut("CTRL+A").unwrap();
        assert_eq!(result.0, KeyCode::Char('a'));
        assert!(result.1.contains(Modifiers::CTRL));
    }

    #[test]
    fn test_empty_string() {
        assert!(parse_shortcut("").is_err());
    }

    #[test]
    fn test_no_key() {
        assert!(parse_shortcut("ctrl+").is_err());
    }

    #[test]
    fn test_duplicate_modifier() {
        assert!(parse_shortcut("ctrl+ctrl+a").is_err());
    }

    #[test]
    fn test_invalid_function_key() {
        assert!(parse_shortcut("ctrl+f13").is_err());
    }

    #[test]
    fn test_digit_key() {
        let result = parse_shortcut("ctrl+5").unwrap();
        assert_eq!(result.0, KeyCode::Char('5'));
        assert!(result.1.contains(Modifiers::CTRL));
    }
}
