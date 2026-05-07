//! Tree-sitter based code analysis using ast-grep
//!
//! Provides language-agnostic symbol extraction and pattern matching.

mod config;
pub(crate) mod pattern_search;
pub(crate) mod symbol_extractor;
pub(crate) mod workspace_analyzer;

pub use config::{
    get_call_node_kinds,
    get_extensions,
    get_import_node_kinds,
    get_symbol_def,
    lang_from_extension,
};

/// Create an ast-grep [`Pattern`] for the given language, catching panics from
/// missing tree-sitter parsers (feature flag not compiled) and converting them
/// to a normal error.
pub(crate) fn try_new_pattern(
    pattern: &str,
    lang: ast_grep_language::SupportLang,
) -> anyhow::Result<ast_grep_core::matcher::Pattern> {
    match std::panic::catch_unwind(|| ast_grep_core::matcher::Pattern::try_new(pattern, lang)) {
        Ok(Ok(p)) => Ok(p),
        Ok(Err(e)) => Err(anyhow::anyhow!("Invalid pattern '{}': {}", pattern, e)),
        Err(payload) => {
            let msg = payload
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| payload.downcast_ref::<String>().map(|s| s.as_str()))
                .unwrap_or("unknown panic");
            Err(anyhow::anyhow!("Language '{}' parser panicked: {}", lang, msg))
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_new_pattern_valid() {
        let lang: ast_grep_language::SupportLang = "javascript".parse().unwrap();
        let result = try_new_pattern("console.log($ARG)", lang);
        assert!(result.is_ok());
    }

    #[test]
    fn try_new_pattern_invalid() {
        let lang: ast_grep_language::SupportLang = "javascript".parse().unwrap();
        // An empty pattern is invalid
        let result = try_new_pattern("", lang);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid pattern"));
    }
}
