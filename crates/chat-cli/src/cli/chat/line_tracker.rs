use serde::{
    Deserialize,
    Serialize,
};

/// Contains metadata for tracking user and agent contribution metrics for a given file for
/// `fs_write` tool uses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileLineTracker {
    /// Line count at the end of the last `fs_write`
    pub prev_fswrite_lines: usize,
    /// Line count before `fs_write` executes
    pub before_fswrite_lines: usize,
    /// Line count after `fs_write` executes
    pub after_fswrite_lines: usize,
    /// Lines added by agent in the current operation
    pub lines_added_by_agent: usize,
    /// Lines removed by agent in the current operation
    pub lines_removed_by_agent: usize,
    /// Whether or not this is the first `fs_write` invocation
    pub is_first_write: bool,
}

impl Default for FileLineTracker {
    fn default() -> Self {
        Self {
            prev_fswrite_lines: 0,
            before_fswrite_lines: 0,
            after_fswrite_lines: 0,
            lines_added_by_agent: 0,
            lines_removed_by_agent: 0,
            is_first_write: true,
        }
    }
}

impl FileLineTracker {
    pub fn lines_by_user(&self) -> isize {
        (self.before_fswrite_lines as isize) - (self.prev_fswrite_lines as isize)
    }

    pub fn lines_by_agent(&self) -> isize {
        (self.lines_added_by_agent + self.lines_removed_by_agent) as isize
    }
}
