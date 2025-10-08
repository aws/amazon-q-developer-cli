use std::time::Duration;

use chrono::{
    DateTime,
    Utc,
};
use rand::Rng as _;
use rand::distr::Alphanumeric;
use serde::{
    Deserialize,
    Serialize,
};
use uuid::Uuid;

use super::agent_loop::protocol::{
    SendRequestArgs,
    UserTurnMetadata,
};
use super::agent_loop::types::Message;
use crate::agent::ExecutionState;
use crate::agent::agent_config::definitions::Config;
use crate::agent::agent_loop::model::ModelsState;
use crate::agent::tools::ToolState;

/// A point-in-time snapshot of an agent's state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSnapshot {
    /// Agent id
    pub id: AgentId,
    /// In-memory modifications to the agent's original config
    pub agent_config: Config,
    /// Agent conversation state
    pub conversation_state: ConversationState,
    /// Agent conversation metadata
    pub conversation_metadata: ConversationMetadata,
    /// History of summaries within the agent
    pub compaction_snapshots: Vec<CompactionSnapshot>,
    /// Agent execution state
    pub execution_state: ExecutionState,
    /// The model used with the agent
    pub model_state: ModelsState,
    /// Persistent state required by tools during the conversation
    pub tool_state: ToolState,
    /// Agent settings
    pub settings: AgentSettings,
}

impl AgentSnapshot {
    pub fn new_empty(agent_config: Config) -> Self {
        Self {
            id: agent_config.name().into(),
            agent_config,
            conversation_state: ConversationState::new(),
            conversation_metadata: Default::default(),
            compaction_snapshots: Default::default(),
            execution_state: Default::default(),
            model_state: Default::default(),
            tool_state: Default::default(),
            settings: Default::default(),
        }
    }

    /// Creates a new snapshot using the built-in agent default.
    pub fn new_built_in_agent() -> Self {
        let agent_config = Config::default();
        Self {
            id: agent_config.name().into(),
            agent_config,
            conversation_state: ConversationState::new(),
            conversation_metadata: Default::default(),
            compaction_snapshots: Default::default(),
            execution_state: Default::default(),
            model_state: Default::default(),
            tool_state: Default::default(),
            settings: Default::default(),
        }
    }
}

// /// A serializable representation of the state contained within [Models].
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub enum ModelsState {
//     Rts {
//         conversation_id: Option<String>,
//         model_id: Option<String>,
//     },
//     Test,
// }
//
// impl Default for ModelsState {
//     fn default() -> Self {
//         Self::Rts {
//             conversation_id: None,
//             model_id: None,
//         }
//     }
// }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionSnapshot {
    conversation_state: ConversationState,
    summary: ConversationSummary,
}

/// Represents a summary of a conversation history.
///
/// Generally created by the model to replace a history of messages with a succinct summarization.
/// Summarizations are done to save tokens by capturing the most important bits of context while
/// removing unnecessary information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationSummary {
    /// Identifier for the summary
    pub id: String,
    /// Conversation summary content
    pub content: String,
    /// Timestamp for when the summary was generated
    #[serde(with = "chrono::serde::ts_seconds_option")]
    pub timestamp: Option<DateTime<Utc>>,
}

/// Settings to modify the runtime behavior of the agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSettings {
    /// Whether or not to automatically perform compaction on context window overflows.
    pub auto_compact: bool,
    /// Timeout waiting for MCP servers to initialize during agent initialization.
    pub mcp_init_timeout: Duration,
}

impl AgentSettings {
    const DEFAULT_MCP_INIT_TIMEOUT: Duration = Duration::from_secs(5);
}

impl Default for AgentSettings {
    fn default() -> Self {
        Self {
            auto_compact: Default::default(),
            mcp_init_timeout: Self::DEFAULT_MCP_INIT_TIMEOUT,
        }
    }
}

/// State associated with a history of messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationState {
    pub id: Uuid,
    pub messages: Vec<Message>,
}

impl ConversationState {
    /// Creates a new conversation state with a new id and empty history.
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4(),
            messages: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConversationMetadata {
    /// History of user turns
    pub user_turn_metadatas: Vec<UserTurnMetadata>,
    /// Summary history
    pub summaries: Vec<ConversationSummary>,
    /// The request that started the most recent user turn
    pub user_turn_start_request: Option<SendRequestArgs>,
    /// The most recent request sent
    ///
    /// This is equivalent to user_turn_start_request for the first request of a user turn
    pub last_request: Option<SendRequestArgs>,
}

/// Unique identifier of an agent instance within a session.
///
/// Formatted as: `parent_id/name#rand`
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentId {
    /// Name of the agent
    ///
    /// This is the same as the agent name in the agent's config
    name: String,
    /// String-formatted id of the agent's parent, if available.
    ///
    /// If available, this would be the result of [AgentId::to_string].
    parent_id: Option<String>,
    /// Random suffix
    rand: Option<String>,
}

impl AgentId {
    // '/', '#', and '|' are not valid characters for an agent name, hence using these as separators.

    const AGENT_ID_SUFFIX: char = '|';
    const RAND_PART_SEPARATOR: char = '#';

    pub fn new(name: String) -> Self {
        Self {
            name,
            parent_id: None,
            rand: Some(rand::rng().sample_iter(&Alphanumeric).take(5).map(char::from).collect()),
        }
    }

    /// Name of the agent, as written in the agent config
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(parent) = self.parent_id.as_ref() {
            write!(f, "{}|", parent)?;
        }
        write!(f, "{}", self.name)?;
        if let Some(id) = self.rand.as_ref() {
            write!(f, "#{}", id)?;
        }
        Ok(())
    }
}

impl<T> From<T> for AgentId
where
    T: AsRef<str>,
{
    fn from(value: T) -> Self {
        let s = value.as_ref();

        let mut parent_part = None;
        let mut rand_part = None;
        if let Some((i, _)) = s.rmatch_indices(Self::AGENT_ID_SUFFIX).next() {
            parent_part = Some((i, s.split_at(i).0.to_string()));
        }
        match (&parent_part, s.rmatch_indices(Self::RAND_PART_SEPARATOR).next()) {
            (Some((i, _)), Some((j, _))) if j > *i => rand_part = Some((j, s.split_at(j + 1).1.to_string())),
            (None, Some((j, _))) => rand_part = Some((j, s.split_at(j + 1).1.to_string())),
            _ => (),
        }
        let name = match (&parent_part, &rand_part) {
            (None, None) => s.split_once(Self::AGENT_ID_SUFFIX).unwrap_or((s, "")).0.to_string(),
            (None, Some((i, _))) => s.split_at(*i).0.to_string(),
            (Some((i, _)), None) => s.split_at(*i + 1).1.to_string(),
            (Some((i, _)), Some((j, _))) => s
                .split_at(*i + 1)
                .1
                .split_at(j.saturating_sub(*i).saturating_sub(1))
                .0
                .to_string(),
        };
        Self {
            name,
            parent_id: parent_part.map(|v| v.1),
            rand: rand_part.map(|v| v.1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_id_parse() {
        macro_rules! assert_agent_id {
            ($val:expr, $s:expr) => {
                assert_eq!($val.to_string(), $s);
                assert_eq!($val, $s.into());
            };
        }

        // Testing as expected in the app
        let parent = AgentId {
            name: "parent".to_string(),
            parent_id: None,
            rand: None,
        };
        assert_agent_id!(parent, "parent");
        let child = AgentId {
            name: "child".to_string(),
            parent_id: Some(parent.to_string()),
            rand: Some("123".to_string()),
        };
        assert_agent_id!(child, "parent|child#123");
        let grandchild = AgentId {
            name: "grandchild".to_string(),
            parent_id: Some(child.to_string()),
            rand: Some("456".to_string()),
        };
        assert_agent_id!(grandchild, "parent|child#123|grandchild#456");

        // Testing edge cases
        let a1 = AgentId {
            name: "a1".to_string(),
            parent_id: None,
            rand: Some("rand".to_string()),
        };
        assert_agent_id!(a1, "a1#rand");
        let a2 = AgentId {
            name: "a2".to_string(),
            parent_id: Some(a1.to_string()),
            rand: None,
        };
        assert_agent_id!(a2, "a1#rand|a2");
        let a3 = AgentId {
            name: "a3".to_string(),
            parent_id: Some(a2.to_string()),
            rand: None,
        };
        assert_agent_id!(a3, "a1#rand|a2|a3");
    }
}
