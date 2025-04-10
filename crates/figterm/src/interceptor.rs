use std::sync::LazyLock;

use anyhow::Result;
use dashmap::DashMap;
use fig_proto::figterm::Action;
use fig_settings::keybindings::{
    KeyBinding,
    KeyBindings,
};
use tracing::trace;

use crate::input::{
    KeyCode,
    KeyEvent,
    Modifiers,
};

// TODO: remove hardcoded list of global actions and use `availability`
const GLOBAL_ACTIONS: &[&str] = &["toggleAutocomplete", "showAutocomplete"];

const IGNORE_ACTION: &str = "ignore";

static ONLY_SHOW_ON_TAB: LazyLock<bool> =
    LazyLock::new(|| fig_settings::settings::get_bool_or("autocomplete.onlyShowOnTab", false));

pub fn key_from_text(text: impl AsRef<str>) -> Option<KeyEvent> {
    let text = text.as_ref();

    let mut modifiers = Modifiers::NONE;
    let mut remaining = text;
    let key_txt = loop {
        match remaining.split_once('+') {
            Some(("", "")) | None => {
                break remaining;
            },
            Some((modifier_txt, key)) => {
                modifiers |= match modifier_txt {
                    "ctrl" | "control" => Modifiers::CTRL,
                    "shift" => Modifiers::SHIFT,
                    "alt" | "option" => Modifiers::ALT,
                    "meta" | "command" => Modifiers::META,
                    _ => Modifiers::NONE,
                };
                remaining = key;
            },
        }
    };

    let key = match key_txt {
        "backspace" => KeyCode::Backspace,
        "enter" => KeyCode::Enter,
        "arrowleft" | "left" => KeyCode::LeftArrow,
        "arrowright" | "right" => KeyCode::RightArrow,
        "arrowup" | "up" => KeyCode::UpArrow,
        "arrowdown" | "down" => KeyCode::DownArrow,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "tab" => KeyCode::Tab,
        // "backtab" => KeyCode::BackTab,
        "delete" => KeyCode::Delete,
        "insert" => KeyCode::Insert,
        "esc" => KeyCode::Escape,
        f_key if f_key.starts_with('f') => {
            let f_key = f_key.trim_start_matches('f');
            let f_key = f_key.parse::<u8>().ok()?;
            KeyCode::Function(f_key)
        },
        c => {
            let mut chars = c.chars();
            let mut first_char = chars.next()?;

            if modifiers.contains(Modifiers::SHIFT) && first_char.is_ascii_lowercase() {
                first_char = first_char.to_ascii_uppercase();
                modifiers.remove(Modifiers::SHIFT);
            }

            if chars.next().is_some() {
                return None;
            }
            KeyCode::Char(first_char)
        },
    };

    Some(KeyEvent { key, modifiers })
}

#[derive(Debug, Clone, Default)]
pub struct KeyInterceptor {
    intercept_global: bool,
    intercept: bool,

    window_visible: bool,

    // TODO: this should be based on `availability`
    _global_actions: Vec<Action>,

    mappings: DashMap<KeyEvent, String, fnv::FnvBuildHasher>,
}

impl KeyInterceptor {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn load_key_intercepts(&mut self) -> Result<()> {
        let key_bindings = KeyBindings::load_hardcoded();
        for KeyBinding { identifier, binding } in key_bindings {
            if let Some(binding) = key_from_text(binding) {
                self.insert_binding(binding, identifier);
            }
        }
        Ok(())
    }

    pub fn set_intercept_global(&mut self, intercept_global: bool) {
        trace!("Setting intercept global to {intercept_global}");
        self.intercept_global = intercept_global;
    }

    pub fn set_intercept(&mut self, intercept: bool) {
        trace!("Setting intercept to {intercept}");
        self.intercept = intercept;
    }

    pub fn set_window_visible(&mut self, window_visible: bool) {
        trace!("Setting window visible to {window_visible}");
        self.window_visible = window_visible;
    }

    pub fn set_actions(&mut self, actions: &[Action], override_actions: bool) {
        if override_actions {
            self.mappings.clear();
        }

        for Action { identifier, bindings } in actions {
            for binding in bindings {
                if let Some(binding) = key_from_text(binding) {
                    self.insert_binding(binding, identifier.clone());
                }
            }
        }
    }

    fn insert_binding(&mut self, binding: KeyEvent, identifier: String) {
        if let Some(key) = match binding.key {
            KeyCode::UpArrow => Some(KeyCode::ApplicationUpArrow),
            KeyCode::DownArrow => Some(KeyCode::ApplicationDownArrow),
            KeyCode::LeftArrow => Some(KeyCode::ApplicationLeftArrow),
            KeyCode::RightArrow => Some(KeyCode::ApplicationRightArrow),
            _ => None,
        } {
            self.mappings.insert(
                KeyEvent {
                    key,
                    modifiers: binding.modifiers,
                },
                identifier.clone(),
            );
        };

        if let KeyCode::Char(key) = binding.key {
            // Fill in other case if there is a ctrl or alt, i.e. ctrl+r is the same as ctrl+R
            //
            // This will prevent ctrl+shift+r from being the same as ctrl+r but that is probably
            // fine since we lose context due to parsing ambiguity in the original xterm spec
            // when other modifiers are present
            if (binding.modifiers.contains(Modifiers::CTRL) || binding.modifiers.contains(Modifiers::ALT))
                && key.is_ascii_alphabetic()
            {
                self.mappings.insert(
                    KeyEvent {
                        key: KeyCode::Char(if key.is_ascii_uppercase() {
                            key.to_ascii_lowercase()
                        } else {
                            key.to_ascii_uppercase()
                        }),
                        modifiers: binding.modifiers,
                    },
                    identifier.clone(),
                );
            }
        }

        self.mappings.insert(binding, identifier);
    }

    pub fn reset(&mut self) {
        trace!("Resetting key interceptor");
        self.intercept_global = false;
        self.intercept = false;
    }

    pub fn intercept_key(&self, key_event: &KeyEvent) -> Option<String> {
        trace!(?key_event, "Intercepting key");

        match (self.intercept_global, self.intercept) {
            (true, false) => {
                // TODO: only show on tab should be encoded in AE
                if key_event.key == KeyCode::Tab && *ONLY_SHOW_ON_TAB {
                    Some("showAutocomplete".into())
                } else {
                    match self.mappings.get(key_event) {
                        Some(action) if action.value() == IGNORE_ACTION => None,
                        Some(action) if GLOBAL_ACTIONS.contains(&action.value().as_str()) => {
                            Some(action.value().clone())
                        },
                        _ => None,
                    }
                }
            },
            (_, true) => {
                if self.window_visible {
                    match self.mappings.get(key_event) {
                        Some(action) if action.value() == IGNORE_ACTION => None,
                        Some(action) => Some(action.value().to_string()),
                        None => None,
                    }
                } else {
                    None
                }
            },
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_from_text() {
        let assert_key = |text: &str, key, modifiers| {
            assert_eq!(key_from_text(text), Some(KeyEvent { key, modifiers }));
        };

        assert_key("a", KeyCode::Char('a'), Modifiers::NONE);
        assert_key("ctrl+a", KeyCode::Char('a'), Modifiers::CTRL);
        assert_key("ctrl+shift+a", KeyCode::Char('A'), Modifiers::CTRL);
        assert_key("backspace", KeyCode::Backspace, Modifiers::NONE);

        // invalid
        assert_eq!(key_from_text("invalid"), None);
        assert_eq!(key_from_text("ctrl+invalid"), None);
    }

    #[test]
    fn test_key_interceptor() {
        let mut interceptor = KeyInterceptor::new();
        interceptor.load_key_intercepts().unwrap();

        assert_eq!(
            interceptor.intercept_key(&KeyEvent {
                key: KeyCode::Tab,
                modifiers: Modifiers::NONE
            }),
            None
        );

        interceptor.set_intercept(true);
        interceptor.set_window_visible(true);

        assert_eq!(
            interceptor.intercept_key(&KeyEvent {
                key: KeyCode::Tab,
                modifiers: Modifiers::NONE
            }),
            Some("insertCommonPrefix".into())
        );
        assert_eq!(
            interceptor.intercept_key(&KeyEvent {
                key: KeyCode::DownArrow,
                modifiers: Modifiers::NONE
            }),
            Some("navigateDown".into())
        );
    }
}
