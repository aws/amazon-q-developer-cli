//! Trust option generation for fs_read and fs_write tools.
//!
//! Generates tiered trust options:
//!   - Tier 1: Exact paths ("Specific paths")
//!   - Tier 2: Parent (single path) or common ancestor (multiple paths) ("Complete directory")

use std::path::Path;

use crate::agent::permissions::PathAccessType;
use crate::agent::protocol::TrustOption;

/// Generate trust options for file operations.
pub fn generate_file_trust_options(
    paths_as_string: &[String],
    access_type: PathAccessType,
    cwd: &Path,
) -> Vec<TrustOption> {
    if paths_as_string.is_empty() {
        return vec![];
    }

    let paths: Vec<&Path> = paths_as_string.iter().map(|p| Path::new(p.as_str())).collect();

    let setting_key = access_type.setting_key();

    let mut options = Vec::new();

    // Tier 1: Exact paths (display relative to cwd if possible)
    let display_paths: Vec<String> = paths
        .iter()
        .map(|p| match p.strip_prefix(cwd) {
            Ok(rel) => rel.to_string_lossy().into_owned(),
            Err(_) => p.to_string_lossy().into_owned(),
        })
        .collect();
    options.push(TrustOption {
        label: "Specific paths".into(),
        display: format_paths_display(&display_paths),
        setting_key: setting_key.into(),
        patterns: paths_as_string.to_vec(),
    });

    // Tier 2: common ancestor
    if let Some(ancestor) = common_ancestor(&paths) {
        options.push(make_dir_option(ancestor, cwd, setting_key));
    }

    options
}

/// Find the longest common ancestor of all paths.
/// Returns None if paths share only the root (/ or empty).
fn common_ancestor<'a>(paths: &[&'a Path]) -> Option<&'a Path> {
    let first = paths.first()?;
    let mut ancestor = first.parent()?;
    for path in &paths[1..] {
        while !path.starts_with(ancestor) {
            ancestor = ancestor.parent()?;
        }
    }
    // Don't return shallow ancestors (e.g., / or C:\)
    if ancestor.components().count() < 3 {
        return None;
    }
    Some(ancestor)
}

fn make_dir_option(dir: &Path, cwd: &Path, setting_key: &str) -> TrustOption {
    let display = match dir.strip_prefix(cwd) {
        Ok(rel) if !rel.as_os_str().is_empty() => rel.to_string_lossy().into_owned(),
        _ => dir.to_string_lossy().into_owned(),
    };
    TrustOption {
        label: "Complete directory".into(),
        display,
        setting_key: setting_key.into(),
        patterns: vec![dir.to_string_lossy().into_owned()],
    }
}

fn format_paths_display(paths: &[String]) -> String {
    const MAX_DISPLAY: usize = 3;
    if paths.len() <= MAX_DISPLAY {
        paths.join(", ")
    } else {
        format!(
            "{}, ... (+{} more)",
            paths[..MAX_DISPLAY].join(", "),
            paths.len() - MAX_DISPLAY
        )
    }
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;

    #[derive(Deserialize)]
    struct TestData {
        test_cases: Vec<TestCase>,
    }

    #[derive(Deserialize)]
    struct TestCase {
        name: String,
        cwd: String,
        paths: Vec<String>,
        #[serde(default = "default_access")]
        access_type: String,
        expected: Vec<TrustOption>,
    }

    fn default_access() -> String {
        "read".to_string()
    }

    #[test]
    fn test_from_json_data() {
        let json_data = include_str!("test_data/file_trust_tests.json");
        let test_data: TestData = serde_json::from_str(json_data).expect("Failed to parse test data");

        for tc in test_data.test_cases {
            let cwd = Path::new(&tc.cwd);
            let access = if tc.access_type == "write" {
                PathAccessType::Write
            } else {
                PathAccessType::Read
            };
            let opts = generate_file_trust_options(&tc.paths, access, cwd);

            assert_eq!(opts, tc.expected, "Test '{}' failed", tc.name);
        }
    }
}
