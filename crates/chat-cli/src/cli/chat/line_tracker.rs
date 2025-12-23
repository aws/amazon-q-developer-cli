use serde::{
    Deserialize,
    Serialize,
};
use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};

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
    /// Pending retention checks scheduled for 1 minute (changed from 15 minutes for testing)
    #[serde(default)]
    pub pending_retention_checks: Vec<RetentionCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionCheck {
    pub lines: Vec<String>,
    pub scheduled_time: u64,
    pub conversation_id: String,
    pub tool_use_id: String,
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
            pending_retention_checks: Vec::new(),
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

    pub fn schedule_retention_check(&mut self, lines: Vec<String>, conversation_id: String, tool_use_id: String) {
        let scheduled_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() + 900; // 15 minutes from now
        
        self.pending_retention_checks.push(RetentionCheck {
            lines,
            scheduled_time,
            conversation_id,
            tool_use_id,
        });
    }

    pub fn flush_pending_checks_for_agent_rewrite(&mut self, file_content: &str) -> Vec<(String, String, usize, usize, String)> {
        let mut results = Vec::new();
        let file_lines: HashSet<&str> = file_content.lines().collect();
        
        for check in self.pending_retention_checks.drain(..) {
            let retained = check.lines.iter()
                .filter(|line| file_lines.contains(line.as_str()))
                .count();
            
            results.push((
                check.conversation_id,
                check.tool_use_id,
                retained,
                check.lines.len(),
                "agent_rewrite".to_string(),
            ));
        }
        
        results
    }

    pub fn check_due_retention(&mut self, file_content: &str) -> Vec<(String, String, usize, usize, String)> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        let mut results = Vec::new();
        let mut remaining_checks = Vec::new();
        let file_lines: HashSet<&str> = file_content.lines().collect();
        
        for check in self.pending_retention_checks.drain(..) {
            if now >= check.scheduled_time {
                let retained = check.lines.iter()
                    .filter(|line| file_lines.contains(line.as_str()))
                    .count();
                
                results.push((
                    check.conversation_id,
                    check.tool_use_id,
                    retained,
                    check.lines.len(),
                    "15min_check".to_string(),
                ));
            } else {
                remaining_checks.push(check);
            }
        }
        
        self.pending_retention_checks = remaining_checks;
        results
    }

    pub fn flush_all_retention_checks(&mut self, file_content: &str, source: &str) -> Vec<(String, String, usize, usize, String)> {
        let mut results = Vec::new();
        let file_lines: HashSet<&str> = file_content.lines().collect();
        
        for check in self.pending_retention_checks.drain(..) {
            let retained = check.lines.iter()
                .filter(|line| file_lines.contains(line.as_str()))
                .count();
            
            results.push((
                check.conversation_id,
                check.tool_use_id,
                retained,
                check.lines.len(),
                source.to_string(),
            ));
        }
        
        results
    }
}
