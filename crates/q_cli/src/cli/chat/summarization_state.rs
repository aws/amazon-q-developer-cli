/// Character count warning levels for conversation size
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenWarningLevel {
    /// No warning, conversation is within normal limits
    None,
    /// Critical level - at single warning threshold (500K characters)
    Critical,
}

/// Constants for character-based warning threshold
pub const MAX_CHARS: usize = 500000; // Character-based warning threshold
