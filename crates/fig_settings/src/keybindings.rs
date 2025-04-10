use std::fmt::Display;

use serde::{
    Deserialize,
    Serialize,
};

use crate::{
    Error,
    JsonStore,
    OldSettings,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Availability {
    WhenFocused,
    Always,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyBindingDescription {
    pub identifier: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub category: Option<String>,
    pub availability: Option<Availability>,
    pub default_bindings: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyBinding {
    pub identifier: String,
    pub binding: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct KeyBindings(pub Vec<KeyBinding>);

impl KeyBindings {
    pub fn load_hardcoded() -> Self {
        let hardcoded_descriptions: Vec<KeyBindingDescription> =
            serde_json::from_str(include_str!("actions.json")).expect("Unable to load hardcoded actions");

        let key_bindings = hardcoded_descriptions
            .into_iter()
            .flat_map(|description| {
                description
                    .default_bindings
                    .unwrap_or_default()
                    .into_iter()
                    .map(move |binding| KeyBinding {
                        identifier: description.identifier.clone(),
                        binding,
                    })
            })
            .collect();

        Self(key_bindings)
    }

    fn load_from_json_map(
        json_map: &serde_json::Map<String, serde_json::Value>,
        product_namespace: impl Display,
    ) -> Self {
        let key_bindings = json_map
            .into_iter()
            .filter_map(|(key, value)| {
                if let Some(key) = key.strip_prefix(&format!("{product_namespace}.keybindings.",)) {
                    Some(KeyBinding {
                        identifier: value.as_str()?.into(),
                        binding: key.into(),
                    })
                } else {
                    None
                }
            })
            .collect();
        Self(key_bindings)
    }

    pub fn load_from_settings(product_namespace: impl Display) -> Result<Self, Error> {
        let settings = OldSettings::load()?;
        let map = settings.map();
        Ok(Self::load_from_json_map(&map, product_namespace))
    }
}

impl IntoIterator for KeyBindings {
    type IntoIter = std::vec::IntoIter<Self::Item>;
    type Item = KeyBinding;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_load_json() {
        let json = KeyBindings::load_hardcoded();
        assert_eq!(json.0.len(), 24);

        assert_eq!(json.0[0].identifier, "insertSelected");
        assert_eq!(json.0[0].binding, "enter");
    }

    #[test]
    fn test_load_from_json_map() {
        let json_map = serde_json::json!({
            "autocomplete.keybindings.command+i": "toggleDescription",
            "autocomplete.keybindings.control+-": "increaseSize",
            "autocomplete.keybindings.control+/": "toggleDescription",
            "autocomplete.keybindings.control+=": "decreaseSize",
            "autocomplete.other": "other",
            "other": "other",
        })
        .as_object()
        .unwrap()
        .clone();

        let json = KeyBindings::load_from_json_map(&json_map, "autocomplete");

        assert_eq!(json.0.len(), 4);

        assert_eq!(json.0[0].identifier, "toggleDescription");
        assert_eq!(json.0[0].binding, "command+i");

        assert_eq!(json.0[1].identifier, "increaseSize");
        assert_eq!(json.0[1].binding, "control+-");

        assert_eq!(json.0[2].identifier, "toggleDescription");
        assert_eq!(json.0[2].binding, "control+/");

        assert_eq!(json.0[3].identifier, "decreaseSize");
        assert_eq!(json.0[3].binding, "control+=");
    }
}
