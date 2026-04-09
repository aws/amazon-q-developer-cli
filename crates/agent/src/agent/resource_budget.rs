use crate::agent::consts::{
    BYTES_PER_TOKEN,
    DEFAULT_CONTEXT_WINDOW_SIZE,
};

/// A resource file loaded from the agent config.
#[derive(Debug)]
pub struct Resource {
    /// Exact value from the config this resource was taken from
    pub config_value: String,
    /// Resource content
    pub content: String,
}

/// Drop entire resource files that exceed the total budget (75% of context window).
/// Files are sorted largest-first so smaller files are preferred when space is limited.
/// This matches V1 behavior where files are included in full or dropped entirely.
pub fn drop_resources_exceeding_budget(files: &mut Vec<Resource>, context_window_size: Option<usize>) {
    let max_resource_bytes = context_window_size.unwrap_or(DEFAULT_CONTEXT_WINDOW_SIZE) * 3 / 4 * BYTES_PER_TOKEN;
    files.sort_by(|a, b| b.content.len().cmp(&a.content.len()));
    let mut total = 0usize;
    files.retain(|f| {
        if total + f.content.len() <= max_resource_bytes {
            total += f.content.len();
            true
        } else {
            tracing::warn!(
                "Dropping resource '{}' ({} bytes) — total resource budget exceeded",
                f.config_value,
                f.content.len()
            );
            false
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_resource(name: &str, size: usize) -> Resource {
        Resource {
            config_value: name.to_string(),
            content: "x".repeat(size),
        }
    }

    #[test]
    fn test_all_files_fit_within_budget() {
        // 100_000 tokens * 3/4 * 4 bytes = 300_000 bytes budget
        let mut files = vec![make_resource("a.md", 100), make_resource("b.md", 200)];
        drop_resources_exceeding_budget(&mut files, Some(100_000));
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn test_large_file_dropped_small_kept() {
        // Budget = 100 tokens * 3/4 * 4 = 300 bytes
        let mut files = vec![make_resource("small.md", 100), make_resource("big.md", 500)];
        drop_resources_exceeding_budget(&mut files, Some(100));
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].config_value, "small.md");
    }

    #[test]
    fn test_drops_largest_first_keeps_smaller() {
        // Budget = 200 tokens * 3/4 * 4 = 600 bytes
        let mut files = vec![
            make_resource("a.md", 400),
            make_resource("b.md", 100),
            make_resource("c.md", 150),
        ];
        drop_resources_exceeding_budget(&mut files, Some(200));
        // Sorted largest-first: a(400), c(150), b(100)
        // a(400) fits (400 <= 600), c(150) fits (550 <= 600), b(100) doesn't (650 > 600)
        assert_eq!(files.len(), 2);
        let names: Vec<&str> = files.iter().map(|f| f.config_value.as_str()).collect();
        assert!(names.contains(&"a.md"));
        assert!(names.contains(&"c.md"));
    }

    #[test]
    fn test_all_files_dropped_when_none_fit() {
        // Budget = 1 token * 3/4 * 4 = 3 bytes
        let mut files = vec![make_resource("a.md", 100)];
        drop_resources_exceeding_budget(&mut files, Some(1));
        assert_eq!(files.len(), 0);
    }

    #[test]
    fn test_uses_default_context_window_when_none() {
        // None -> DEFAULT_CONTEXT_WINDOW_SIZE (200_000) * 3/4 * 4 = 600_000 bytes
        let mut files = vec![make_resource("a.md", 400_000), make_resource("b.md", 400_000)];
        drop_resources_exceeding_budget(&mut files, None);
        // Only one fits
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn test_empty_files_vec() {
        let mut files: Vec<Resource> = vec![];
        drop_resources_exceeding_budget(&mut files, Some(100_000));
        assert_eq!(files.len(), 0);
    }
}
