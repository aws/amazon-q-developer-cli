use std::ops::Deref;

use aws_smithy_types::Document;
use fig_api_client::model::{
    AssistantResponseMessage,
    ToolResultContentBlock,
    UserInputMessage,
};

use super::conversation_state::{
    BackendConversationState,
    ConversationSize,
};

#[derive(Debug, Clone, Copy)]
pub struct CharCount(usize);

impl CharCount {
    pub fn value(&self) -> usize {
        self.0
    }
}

impl Deref for CharCount {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<usize> for CharCount {
    fn from(value: usize) -> Self {
        Self(value)
    }
}

impl std::ops::Add for CharCount {
    type Output = CharCount;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.value() + rhs.value())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TokenCount(usize);

impl TokenCount {
    pub fn value(&self) -> usize {
        self.0
    }
}

impl Deref for TokenCount {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<CharCount> for TokenCount {
    fn from(value: CharCount) -> Self {
        Self(TokenCounter::count_tokens_char_count(value.value()))
    }
}

impl std::fmt::Display for TokenCount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub struct TokenCounter;

impl TokenCounter {
    pub const TOKEN_TO_CHAR_RATIO: usize = 3;

    /// Estimates the number of tokens in the input content.
    /// Currently uses a simple heuristic: content length / TOKEN_TO_CHAR_RATIO
    ///
    /// Rounds up to the nearest multiple of 10 to avoid giving users a false sense of precision.
    pub fn count_tokens(content: &str) -> usize {
        Self::count_tokens_char_count(content.len())
    }

    fn count_tokens_char_count(count: usize) -> usize {
        (count / Self::TOKEN_TO_CHAR_RATIO + 5) / 10 * 10
    }

    pub const fn token_to_chars(token: usize) -> usize {
        token * Self::TOKEN_TO_CHAR_RATIO
    }
}

/// A trait for types that represent some number of characters (aka bytes). For use in calculating
/// context window size utilization.
pub trait CharCounter {
    /// Returns the number of characters contained within this type.
    ///
    /// One "character" is essentially the same as one "byte"
    fn char_count(&self) -> CharCount;
}

impl CharCounter for BackendConversationState<'_> {
    fn char_count(&self) -> CharCount {
        self.get_utilization().char_count()
    }
}

impl CharCounter for ConversationSize {
    fn char_count(&self) -> CharCount {
        self.user_messages + self.assistant_messages + self.context_messages
    }
}

impl CharCounter for UserInputMessage {
    fn char_count(&self) -> CharCount {
        let mut total_chars = 0;
        total_chars += self.content.len();
        if let Some(ctx) = &self.user_input_message_context {
            // Add tool result characters if any
            if let Some(results) = &ctx.tool_results {
                for result in results {
                    for content in &result.content {
                        match content {
                            ToolResultContentBlock::Text(text) => {
                                total_chars += text.len();
                            },
                            ToolResultContentBlock::Json(doc) => {
                                total_chars += calculate_document_char_count(doc);
                            },
                        }
                    }
                }
            }
        }
        total_chars.into()
    }
}

impl CharCounter for AssistantResponseMessage {
    fn char_count(&self) -> CharCount {
        let mut total_chars = 0;
        total_chars += self.content.len();
        if let Some(tool_uses) = &self.tool_uses {
            total_chars += tool_uses
                .iter()
                .map(|v| calculate_document_char_count(&v.input))
                .reduce(|acc, e| acc + e)
                .unwrap_or_default();
        }
        total_chars.into()
    }
}

fn calculate_document_char_count(document: &Document) -> usize {
    match document {
        Document::Object(hash_map) => hash_map
            .values()
            .fold(0, |acc, e| acc + calculate_document_char_count(e)),
        Document::Array(vec) => vec.iter().fold(0, |acc, e| acc + calculate_document_char_count(e)),
        Document::Number(_) => 1,
        Document::String(s) => s.len(),
        Document::Bool(_) => 1,
        Document::Null => 1,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use aws_smithy_types::Number;

    use super::*;

    #[test]
    fn test_token_count() {
        let text = "This is a test sentence.";
        let count = TokenCounter::count_tokens(text);
        assert_eq!(count, (text.len() / 3 + 5) / 10 * 10);
    }

    #[test]
    fn test_calculate_document_char_count() {
        // Test simple types
        assert_eq!(calculate_document_char_count(&Document::String("hello".to_string())), 5);
        assert_eq!(calculate_document_char_count(&Document::Number(Number::PosInt(123))), 1);
        assert_eq!(calculate_document_char_count(&Document::Bool(true)), 1);
        assert_eq!(calculate_document_char_count(&Document::Null), 1);

        // Test array
        let array = Document::Array(vec![
            Document::String("test".to_string()),
            Document::Number(Number::PosInt(42)),
            Document::Bool(false),
        ]);
        assert_eq!(calculate_document_char_count(&array), 6); // "test" (4) + Number (1) + Bool (1)

        // Test object
        let mut obj = HashMap::new();
        obj.insert("key1".to_string(), Document::String("value1".to_string()));
        obj.insert("key2".to_string(), Document::Number(Number::PosInt(99)));
        let object = Document::Object(obj);
        assert_eq!(calculate_document_char_count(&object), 7); // "value1" (6) + Number (1)

        // Test nested structure
        let mut nested_obj = HashMap::new();
        let mut inner_obj = HashMap::new();
        inner_obj.insert("inner_key".to_string(), Document::String("inner_value".to_string()));
        nested_obj.insert("outer_key".to_string(), Document::Object(inner_obj));
        nested_obj.insert(
            "array_key".to_string(),
            Document::Array(vec![
                Document::String("item1".to_string()),
                Document::String("item2".to_string()),
            ]),
        );

        let complex = Document::Object(nested_obj);
        assert_eq!(calculate_document_char_count(&complex), 21); // "inner_value" (11) + "item1" (5) + "item2" (5)

        // Test empty structures
        assert_eq!(calculate_document_char_count(&Document::Array(vec![])), 0);
        assert_eq!(calculate_document_char_count(&Document::Object(HashMap::new())), 0);
    }
}
