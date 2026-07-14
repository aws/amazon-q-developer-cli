//! V2 (CLI) tool -> V3 (KAS) selectors: a built-in's `tools` tag and its `permissions` capability.
//! V2 spellings/aliases resolve through `BuiltInToolName` rather than being re-declared; this file
//! owns only the V3-specific tag/capability grouping (e.g. `grep`/`glob`/`code` fold into `read`).

use std::str::FromStr;

use crate::agent::tools::BuiltInToolName;

/// A built-in's `(tools tag, permissions capability)`; capability is `None` for no-policy tools.
fn v3_mapping(tool: BuiltInToolName) -> Option<(&'static str, Option<&'static str>)> {
    use BuiltInToolName::{
        AgentCrew,
        Code,
        ExecuteCmd,
        FsRead,
        FsWrite,
        Glob,
        Grep,
        Introspect,
        Knowledge,
        Task,
        WebFetch,
        WebSearch,
    };
    let mapping = match tool {
        FsRead | Grep | Glob | Code | Introspect => ("read", Some("fs_read")),
        FsWrite => ("write", Some("fs_write")),
        ExecuteCmd => ("shell", Some("shell")),
        WebFetch => ("web", Some("web_fetch")),
        WebSearch => ("web", Some("web_search")),
        AgentCrew => ("subagent", Some("subagent")),
        Knowledge => ("knowledge", None),
        Task => ("todo_list", None),
        _ => return None,
    };
    Some(mapping)
}

fn canonical(name: &str) -> Option<BuiltInToolName> {
    BuiltInToolName::from_str(name).ok()
}

/// V2 tool name -> its V3 `tools` tag (absent for unknown/unmapped names).
pub fn v3_tag_by_v2_name(name: &str) -> Option<&'static str> {
    v3_mapping(canonical(name)?).map(|(tag, _)| tag)
}

/// V2 tool name -> its V3 permissions capability (absent for no-policy tools and unknown names).
pub fn v3_cap_by_v2_name(name: &str) -> Option<&'static str> {
    v3_mapping(canonical(name)?).and_then(|(_, cap)| cap)
}

/// All accepted V2 spellings for the same tool as `name` (aliases plus the display name), or just
/// `name` when it isn't a known built-in.
pub fn aliases_of_v2_name(name: &str) -> Vec<String> {
    match canonical(name) {
        Some(tool) => {
            let mut names: Vec<String> = tool
                .aliases()
                .map(|a| a.iter().map(|s| s.to_string()).collect())
                .unwrap_or_default();
            // `aliases()` omits the display spelling (e.g. `read` for FsRead); include it.
            let display = tool.to_string();
            if !names.contains(&display) {
                names.push(display);
            }
            names
        },
        None => vec![name.to_string()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v3_tag_resolves_aliases_and_folds_read_family() {
        // Different spellings of one tool resolve to the same tag (alias resolution).
        assert_eq!(v3_tag_by_v2_name("fs_read"), Some("read"));
        assert_eq!(v3_tag_by_v2_name("fsRead"), Some("read"));
        // The grouping this file owns: grep/glob/code fold into the `read` tag.
        assert_eq!(v3_tag_by_v2_name("grep"), Some("read"));
        assert_eq!(v3_tag_by_v2_name("code"), Some("read"));
        // A tag whose name differs from the tool spelling.
        assert_eq!(v3_tag_by_v2_name("execute_bash"), Some("shell"));
    }

    #[test]
    fn v3_tag_unknown_is_none() {
        assert_eq!(v3_tag_by_v2_name("nope"), None);
    }

    #[test]
    fn v3_cap_present_for_policy_tools_absent_for_no_policy() {
        // Policy tool: has a capability, which may differ from the tool name (crew -> subagent).
        assert_eq!(v3_cap_by_v2_name("fs_read"), Some("fs_read"));
        assert_eq!(v3_cap_by_v2_name("agent_crew"), Some("subagent"));
        // No-policy tool: no capability.
        assert_eq!(v3_cap_by_v2_name("knowledge"), None);
    }

    #[test]
    fn v3_cap_unknown_is_none() {
        assert_eq!(v3_cap_by_v2_name("nope"), None);
    }

    #[test]
    fn aliases_include_display_and_alias_spellings() {
        let read = aliases_of_v2_name("read");
        assert!(read.contains(&"read".to_string()));
        assert!(read.contains(&"fs_read".to_string()));

        // Any spelling resolves to the same alias set.
        assert_eq!(
            {
                let mut a = aliases_of_v2_name("fsRead");
                a.sort();
                a
            },
            {
                let mut a = aliases_of_v2_name("read");
                a.sort();
                a
            }
        );
    }

    #[test]
    fn aliases_unknown_returns_self() {
        assert_eq!(aliases_of_v2_name("custom_tool"), vec!["custom_tool".to_string()]);
    }
}
