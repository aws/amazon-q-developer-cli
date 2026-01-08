//! Language configuration for symbol extraction (embedded JSON)

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;

const LANGUAGES_JSON: &str = include_str!("languages/languages.json");

#[derive(Debug, Deserialize)]
pub struct SymbolDef {
    pub node_kind: String,
    pub symbol_type: String,
    pub name_child: String,
}

#[derive(Debug, Deserialize)]
pub struct CallDef {
    pub node_kind: String,
}

#[derive(Debug, Deserialize)]
pub struct ImportDef {
    pub node_kind: String,
}

#[derive(Debug, Deserialize)]
pub struct LanguageDef {
    pub extensions: Vec<String>,
    pub symbols: Vec<SymbolDef>,
    #[serde(default)]
    pub calls: Vec<CallDef>,
    #[serde(default)]
    pub imports: Vec<ImportDef>,
}

pub type LanguageConfig = HashMap<String, LanguageDef>;

static CONFIG: OnceLock<LanguageConfig> = OnceLock::new();

pub fn get_config() -> &'static LanguageConfig {
    CONFIG.get_or_init(|| serde_json::from_str(LANGUAGES_JSON).expect("Invalid languages.json"))
}

pub fn get_symbol_def(lang: &str, node_kind: &str) -> Option<&'static SymbolDef> {
    get_config()
        .get(lang)?
        .symbols
        .iter()
        .find(|s| s.node_kind == node_kind)
}

pub fn get_call_node_kinds(lang: &str) -> Vec<&'static str> {
    get_config()
        .get(lang)
        .map(|l| l.calls.iter().map(|c| c.node_kind.as_str()).collect())
        .unwrap_or_default()
}

pub fn get_import_node_kinds(lang: &str) -> Vec<&'static str> {
    get_config()
        .get(lang)
        .map(|l| l.imports.iter().map(|i| i.node_kind.as_str()).collect())
        .unwrap_or_default()
}

pub fn get_extensions(lang: &str) -> Option<&'static [String]> {
    get_config().get(lang).map(|l| l.extensions.as_slice())
}

pub fn lang_from_extension(ext: &str) -> Option<&'static str> {
    let ext_lower = ext.to_lowercase();
    get_config()
        .iter()
        .find(|(_, def)| def.extensions.iter().any(|e| e == &ext_lower))
        .map(|(lang, _)| lang.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_config() {
        let config = get_config();
        assert!(config.contains_key("rust"));
        assert!(config.contains_key("typescript"));
    }

    #[test]
    fn test_get_symbol_def() {
        let def = get_symbol_def("rust", "function_item");
        assert!(def.is_some());
        assert_eq!(def.unwrap().symbol_type, "Function");
    }

    #[test]
    fn test_lang_from_extension() {
        assert_eq!(lang_from_extension("rs"), Some("rust"));
        assert_eq!(lang_from_extension("ts"), Some("typescript"));
    }
}
