use std::collections::HashMap;

use crossterm::event::{
    KeyCode,
    KeyEvent,
    KeyEventKind,
    KeyEventState,
    KeyModifiers,
};
use serde::{
    Deserialize,
    Deserializer,
    Serialize,
};

use super::action::{
    Action,
    Scroll,
    ScrollDistance,
};

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Mode {
    #[default]
    Default,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub keybindings: KeyBindings,
}

#[derive(Clone, Debug)]
pub struct KeyBindings(pub HashMap<Mode, HashMap<Vec<KeyEvent>, Action>>);

impl Default for KeyBindings {
    fn default() -> Self {
        let mut mapping = HashMap::<Vec<KeyEvent>, Action>::new();
        mapping.insert(
            vec![KeyEvent {
                code: KeyCode::Char('p'),
                modifiers: KeyModifiers::CONTROL,
                kind: KeyEventKind::Press,
                state: KeyEventState::NONE,
            }],
            Action::Scroll(Scroll::Up(ScrollDistance::Message)),
        );
        mapping.insert(
            vec![KeyEvent {
                code: KeyCode::Char('n'),
                modifiers: KeyModifiers::CONTROL,
                kind: KeyEventKind::Press,
                state: KeyEventState::NONE,
            }],
            Action::Scroll(Scroll::Down(ScrollDistance::Message)),
        );

        let mut inner = HashMap::<Mode, _>::new();
        inner.insert(Default::default(), mapping);

        KeyBindings(inner)
    }
}

impl<'de> Deserialize<'de> for KeyBindings {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let parsed_map = HashMap::<Mode, HashMap<String, Action>>::deserialize(deserializer)?;

        let keybindings = parsed_map
            .into_iter()
            .map(|(mode, inner_map)| {
                let converted_inner_map = inner_map
                    .into_iter()
                    .map(|(key_str, cmd)| (parse_key_sequence(&key_str).unwrap(), cmd))
                    .collect();
                (mode, converted_inner_map)
            })
            .collect();

        Ok(KeyBindings(keybindings))
    }
}

// TODO: implement this
pub fn parse_key_sequence(_raw: &str) -> Result<Vec<KeyEvent>, String> {
    Ok(Default::default())
}
