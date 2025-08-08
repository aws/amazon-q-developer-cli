use serde::{
    Deserialize,
    Serialize,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileLineTracker {
    pub prev_lines: usize,
    pub curr_lines: usize,
    pub after_lines: usize,
    pub is_first_write: bool,
}

impl Default for FileLineTracker {
    fn default() -> Self {
        Self {
            prev_lines: 0,
            curr_lines: 0,
            after_lines: 0,
            is_first_write: true,
        }
    }
}

impl FileLineTracker {

    pub fn lines_by_user(&self) -> isize {
        (self.curr_lines as isize) - (self.prev_lines as isize)
    }

    pub fn lines_by_agent(&self) -> isize {
        (self.after_lines as isize) - (self.curr_lines as isize)
    }
}