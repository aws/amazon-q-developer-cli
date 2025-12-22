use clap::Subcommand;
use crossterm::execute;
use crossterm::style::{
    self,
    Stylize,
};
use dialoguer::Select;
use eyre::Result;

use crate::cli::ConversationState;
use crate::cli::chat::context::ContextFilePath;
use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::os::Os;
use crate::theme::StyledText;

/// Display entry for chat session selection
pub struct ChatSessionDisplayEntry {
    pub session_id: String,
    pub timestamp: String,
    pub summary: String,
    pub msg_count: usize,
}

/// Format conversation summary from user prompts
fn format_conversation_summary(conv_state: &ConversationState) -> String {
    conv_state.get_user_prompts().last().map_or_else(
        || "(empty conversation)".to_string(),
        |s| {
            let single_line = s.replace(['\n', '\r'], " ");
            if single_line.chars().count() > 150 {
                let truncated: String = single_line.chars().take(150).collect();
                format!("{truncated}...")
            } else {
                single_line
            }
        },
    )
}

/// Build display entries from conversation list
pub fn build_session_entries(
    conversations: Vec<(String, ConversationState, i64, i64)>,
) -> Vec<ChatSessionDisplayEntry> {
    conversations
        .into_iter()
        .map(
            |(conv_id, conv_state, _created_at, updated_at)| ChatSessionDisplayEntry {
                session_id: conv_id,
                timestamp: format_timestamp(updated_at),
                summary: format_conversation_summary(&conv_state),
                msg_count: conv_state.history().len(),
            },
        )
        .collect()
}

impl std::fmt::Display for ChatSessionDisplayEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} | {} | {} msgs", self.timestamp, self.summary, self.msg_count)
    }
}

/// Commands for managing chat sessions
#[deny(missing_docs)]
#[derive(Debug, PartialEq, Subcommand)]
pub enum ChatSubcommand {
    /// Resume a saved chat session
    Resume,
    /// Save the current chat session to a file
    Save {
        /// Path where the chat session will be saved
        path: String,
        #[arg(short, long)]
        /// Force overwrite if file already exists
        force: bool,
    },
    /// Load a previous chat session from a file
    Load {
        /// Path to the chat session file to load
        path: String,
    },
    /// Save the current chat session using a custom script that receives conversation JSON via
    /// stdin
    #[command(name = "save-via-script")]
    ScriptSave {
        /// Path to script (should exit 0 on success)
        script: String,
    },
    /// Load a chat session using a custom script that outputs conversation JSON to stdout
    #[command(name = "load-via-script")]
    ScriptLoad {
        /// Path to script (should exit 0 on success)
        script: String,
    },
}

impl ChatSubcommand {
    pub async fn execute(self, os: &mut Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        macro_rules! tri {
            ($v:expr, $name:expr, $path:expr) => {
                match $v {
                    Ok(v) => v,
                    Err(err) => {
                        execute!(
                            session.stderr,
                            StyledText::error_fg(),
                            style::Print(format!("\nFailed to {} {}: {}\n\n", $name, $path, &err)),
                            StyledText::reset_attributes()
                        )?;

                        return Ok(ChatState::PromptUser {
                            skip_printing_tools: true,
                        });
                    },
                }
            };
        }

        match self {
            Self::Resume => {
                return resume_chat_session(os, session);
            },
            Self::Save { path, force } => {
                let contents = tri!(serde_json::to_string_pretty(&session.conversation), "export to", &path);
                if os.fs.exists(&path) && !force {
                    execute!(
                        session.stderr,
                        StyledText::error_fg(),
                        style::Print(format!(
                            "\nFile at {} already exists. To overwrite, use -f or --force\n\n",
                            &path
                        )),
                        StyledText::reset_attributes()
                    )?;
                    return Ok(ChatState::PromptUser {
                        skip_printing_tools: true,
                    });
                }
                tri!(os.fs.write(&path, contents).await, "export to", &path);

                execute!(
                    session.stderr,
                    StyledText::success_fg(),
                    style::Print(format!("\n✔ Exported chat session state to {}\n", &path)),
                    StyledText::reset_attributes(),
                    style::Print(format!("To restore this session later, use: /chat load {}\n", &path))
                )?;
            },
            Self::ScriptSave { script } => {
                match script_save(&script, session) {
                    Ok(_) => {
                        execute!(
                            session.stderr,
                            StyledText::success_fg(),
                            style::Print(format!("\n✔ Saved chat session via script {}\n", &script)),
                            StyledText::reset_attributes(),
                            style::Print("To restore this session later, use: /chat load-via-script <load-script>\n"),
                        )?;
                    },
                    Err(_) => {
                        // Silent failure - script can print its own errors to stderr
                    },
                }
            },
            Self::Load { path } => {
                // Try the original path first
                let original_result = os.fs.read_to_string(&path).await;

                // If the original path fails and doesn't end with .json, try with .json appended
                let contents = if original_result.is_err() && !path.ends_with(".json") {
                    let json_path = format!("{path}.json");
                    match os.fs.read_to_string(&json_path).await {
                        Ok(content) => content,
                        Err(_) => {
                            // If both paths fail, return the original error for better user experience
                            tri!(original_result, "import from", &path)
                        },
                    }
                } else {
                    tri!(original_result, "import from", &path)
                };

                let new_state: ConversationState = tri!(serde_json::from_str(&contents), "import from", &path);
                let chat_state = restore_conversation_state(session, new_state);

                execute!(
                    session.stderr,
                    StyledText::success_fg(),
                    style::Print(format!("\n✔ Imported chat session state from {}\n\n", &path)),
                    StyledText::reset_attributes()
                )?;

                return Ok(chat_state);
            },
            Self::ScriptLoad { script } => {
                match script_load(&script) {
                    Ok(new_state) => {
                        let chat_state = restore_conversation_state(session, new_state);
                        execute!(
                            session.stderr,
                            StyledText::success_fg(),
                            style::Print(format!("\n✔ Loaded chat session via script {}\n\n", &script)),
                            StyledText::reset_attributes()
                        )?;

                        return Ok(chat_state);
                    },
                    Err(_) => {
                        // Silent failure - script can print its own errors to stderr
                    },
                }
            },
        }

        Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        })
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Resume => "resume",
            Self::Save { .. } => "save",
            Self::Load { .. } => "load",
            Self::ScriptSave { .. } => "save-via-script",
            Self::ScriptLoad { .. } => "load-via-script",
        }
    }
}

/// Restores a conversation state into the current session, preserving session-specific fields
/// Returns a ChatState that will trigger a conversation summary
fn restore_conversation_state(session: &mut ChatSession, mut new_state: ConversationState) -> ChatState {
    std::mem::swap(&mut new_state.tool_manager, &mut session.conversation.tool_manager);
    std::mem::swap(&mut new_state.mcp_enabled, &mut session.conversation.mcp_enabled);
    std::mem::swap(&mut new_state.model_info, &mut session.conversation.model_info);

    // For context, we would only take paths that are not in the current agent
    // And we'll place them as temporary context
    // Note that we are NOT doing the same with hooks because hooks are more
    // instrinsically linked to agent and it affects the behavior of an agent
    if let Some(cm) = &new_state.context_manager {
        if let Some(existing_cm) = &mut session.conversation.context_manager {
            let existing_paths = &mut existing_cm.paths;
            for incoming_path in &cm.paths {
                if !existing_paths.contains(incoming_path) {
                    existing_paths.push(ContextFilePath::Session(incoming_path.get_path_as_str().to_string()));
                }
            }
        }
    }

    std::mem::swap(
        &mut new_state.context_manager,
        &mut session.conversation.context_manager,
    );
    std::mem::swap(&mut new_state.agents, &mut session.conversation.agents);
    session.conversation = new_state;

    ChatState::HandleInput {
        input: "In a few words, summarize our conversation so far.".to_owned(),
    }
}

fn format_timestamp(timestamp_ms: i64) -> String {
    use chrono::{
        TimeZone,
        Utc,
    };

    let datetime = Utc.timestamp_millis_opt(timestamp_ms).single();
    match datetime {
        Some(dt) => {
            let now = Utc::now();
            let diff = now.signed_duration_since(dt);

            let secs = diff.num_seconds();
            if secs < 60 {
                format!("{secs} seconds ago")
            } else if secs < 3600 {
                let mins = secs / 60;
                format!("{mins} minutes ago")
            } else if secs < 86400 {
                let hours = secs / 3600;
                format!("{hours} hours ago")
            } else {
                let days = secs / 86400;
                format!("{days} days ago")
            }
        },
        None => "unknown".to_string(),
    }
}

/// List all chat sessions for the current directory to a writer.
pub fn list_conversations(os: &Os, writer: &mut impl std::io::Write) -> Result<(), ChatError> {
    let cwd = match std::env::current_dir() {
        Ok(path) => path,
        Err(_) => return Ok(()),
    };

    let conversations = match os.database.list_conversations_by_path(&cwd) {
        Ok(convs) => convs,
        Err(_) => return Ok(()),
    };

    if conversations.is_empty() {
        execute!(
            writer,
            StyledText::info_fg(),
            style::Print(format!("No saved chat sessions for {}\n", cwd.display())),
            StyledText::reset(),
        )?;
        return Ok(());
    }

    execute!(
        writer,
        StyledText::info_fg(),
        style::Print(format!("\nChat sessions for {}:\n\n", cwd.display())),
        StyledText::reset(),
    )?;

    for (conv_id, conv_state, _created_at, updated_at) in conversations {
        let summary = format_conversation_summary(&conv_state);
        let msg_count = conv_state.history().len();
        let timestamp = format_timestamp(updated_at);

        execute!(
            writer,
            style::Print("Chat SessionId: "),
            StyledText::brand_fg(),
            style::Print(format!("{conv_id}\n")),
            StyledText::reset_attributes(),
            style::Print(format!(
                "  {} | {} | {}\n\n",
                timestamp.dim(),
                summary,
                format!("{msg_count} msgs").dim()
            )),
        )?;
    }

    execute!(
        writer,
        style::Print("To delete a session, use: kiro-cli chat --delete-session <SESSION_ID>\n\n")
    )?;
    Ok(())
}

fn script_save(script: &str, session: &ChatSession) -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write;

    let json = serde_json::to_string_pretty(&session.conversation)?;

    let mut child = std::process::Command::new(script)
        .stdin(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(json.as_bytes())?;
    }

    let status = child.wait()?;
    if !status.success() {
        return Err("Script exited with non-zero status".into());
    }

    Ok(())
}

fn script_load(script: &str) -> Result<ConversationState, Box<dyn std::error::Error>> {
    let output = std::process::Command::new(script)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .output()?;

    if !output.status.success() {
        return Err("Script exited with non-zero status".into());
    }

    let json = String::from_utf8(output.stdout)?;
    let state = serde_json::from_str(&json)?;
    Ok(state)
}

pub fn select_chat_session(entries: &[ChatSessionDisplayEntry], prompt_str: &str) -> Option<usize> {
    Select::with_theme(&crate::util::dialoguer_theme())
        .with_prompt(prompt_str)
        .items(entries)
        .default(0)
        .report(false)
        .interact_opt()
        .unwrap_or(None)
}

fn resume_chat_session(os: &Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
    let result = (|| -> Option<ChatState> {
        let cwd = std::env::current_dir()
            .inspect_err(|_| {
                execute!(
                    session.stderr,
                    StyledText::error_fg(),
                    style::Print("\nFailed to get current directory\n\n"),
                    StyledText::reset_attributes()
                )
                .ok();
            })
            .ok()?;

        let conversations = os
            .database
            .list_conversations_by_path(&cwd)
            .inspect_err(|_| {
                execute!(
                    session.stderr,
                    StyledText::error_fg(),
                    style::Print("\nFailed to list chat sessions\n\n"),
                    StyledText::reset_attributes()
                )
                .ok();
            })
            .ok()?;

        if conversations.is_empty() {
            execute!(
                session.stderr,
                StyledText::info_fg(),
                style::Print(format!("\nNo saved chat sessions for {}\n\n", cwd.display())),
                StyledText::reset_attributes()
            )
            .ok();
            return None;
        }

        let entries = build_session_entries(conversations);
        let prompt = "Select a chat session to resume:";

        let index = select_chat_session(&entries, prompt)?;
        let selected_id = &entries[index].session_id;

        let new_state = os
            .database
            .get_conversation_by_id(selected_id)
            .inspect_err(|_| {
                execute!(
                    session.stderr,
                    StyledText::error_fg(),
                    style::Print(format!("\nFailed to retrieve chat session '{selected_id}'\n\n")),
                    StyledText::reset_attributes()
                )
                .ok();
            })
            .ok()?
            .or_else(|| {
                execute!(
                    session.stderr,
                    StyledText::error_fg(),
                    style::Print(format!("\nChat session '{selected_id}' not found\n\n")),
                    StyledText::reset_attributes()
                )
                .ok();
                None
            })?;

        let chat_state = restore_conversation_state(session, new_state);

        execute!(
            session.stderr,
            StyledText::success_fg(),
            style::Print(format!("\n✔ Resumed chat session {selected_id}\n\n")),
            StyledText::reset_attributes()
        )
        .ok()?;

        Some(chat_state)
    })();

    Ok(result.unwrap_or(ChatState::PromptUser {
        skip_printing_tools: true,
    }))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::cli::agent::Agents;
    use crate::cli::chat::ChatSession;
    use crate::cli::chat::input_source::InputSource;
    use crate::cli::chat::tool_manager::ToolManager;
    use crate::cli::chat::tools::ToolSpec;
    use crate::os::Os;

    /// Helper to create a test chat session with minimal setup
    async fn create_test_session(
        os: &mut Os,
        user_inputs: Vec<&str>,
        mock_responses: Vec<&str>,
        resume_conversation_id: Option<String>,
    ) -> (String, ChatSession) {
        os.client.set_mock_output(serde_json::json!([mock_responses]));

        let agents = Agents::default();
        let tool_manager = ToolManager::default();
        let tool_config = serde_json::from_str::<HashMap<String, ToolSpec>>(include_str!("../tools/tool_index.json"))
            .expect("Tools failed to load");

        let conversation_id = uuid::Uuid::new_v4().to_string();
        let mut session = ChatSession::new(
            os,
            &conversation_id,
            agents,
            None,
            InputSource::new_mock(user_inputs.iter().map(|s| s.to_string()).collect()),
            resume_conversation_id,
            || Some(80),
            tool_manager,
            None,
            tool_config,
            true,
            false,
            None,
            false, // mcp_api_failure
            None,  // wrap: Option<WrapMode>
            None,  // registry_data
        )
        .await
        .unwrap();

        session.spawn(os).await.unwrap();

        (conversation_id, session)
    }

    #[tokio::test]
    async fn test_auto_save_and_resume() {
        let mut os = Os::new().await.unwrap();
        let user_input = "who is the 16th president of US";

        let (conversation_id, _session) = create_test_session(
            &mut os,
            vec![user_input, "exit"],
            vec!["Abraham Lincoln was the 16th president of the United States."],
            None,
        )
        .await;

        let (_, resumed_session) = create_test_session(
            &mut os,
            vec!["confirm", "exit"],
            vec!["Yes, correct!"],
            Some(conversation_id),
        )
        .await;

        assert!(
            resumed_session
                .conversation
                .get_user_prompts()
                .iter()
                .any(|p| p.contains(user_input)),
            "Resumed session should contain original message"
        );

        // Create a second conversation for the same path
        let user_input2 = "what is the capital of France";
        let (conversation_id2, _session2) = create_test_session(
            &mut os,
            vec![user_input2, "exit"],
            vec!["Paris is the capital of France."],
            None,
        )
        .await;

        // Resume the second conversation
        let (_, resumed_session2) = create_test_session(&mut os, vec!["exit"], vec![], Some(conversation_id2)).await;

        assert!(
            resumed_session2
                .conversation
                .get_user_prompts()
                .iter()
                .any(|p| p.contains(user_input2)),
            "Second resumed session should contain its message"
        );
        assert!(
            !resumed_session2
                .conversation
                .get_user_prompts()
                .iter()
                .any(|p| p.contains(user_input)),
            "Second resumed session should not contain first session's message"
        );
    }

    #[tokio::test]
    async fn test_list_sessions() {
        use super::list_conversations;

        let mut os = Os::new().await.unwrap();

        let (_, _) = create_test_session(&mut os, vec!["first message", "exit"], vec!["response 1"], None).await;

        let (_, _) = create_test_session(&mut os, vec!["second message", "exit"], vec!["response 2"], None).await;

        let mut output = Vec::new();
        list_conversations(&os, &mut output).unwrap();
        let output_str = String::from_utf8(output).unwrap();

        assert!(output_str.contains("first message"));
        assert!(output_str.contains("second message"));
        assert!(output_str.contains("Chat SessionId:"));
    }

    #[tokio::test]
    async fn test_list_sessions_empty() {
        use super::list_conversations;

        let os = Os::new().await.unwrap();
        let mut output = Vec::new();
        list_conversations(&os, &mut output).unwrap();
        let output_str = String::from_utf8(output).unwrap();

        assert!(output_str.contains("No saved chat sessions"));
    }

    #[tokio::test]
    async fn test_build_session_entries() {
        use super::build_session_entries;

        let mut os = Os::new().await.unwrap();
        let cwd = std::env::current_dir().unwrap();

        let (_, _) = create_test_session(&mut os, vec!["test message", "exit"], vec!["response"], None).await;

        let conversations = os.database.list_conversations_by_path(&cwd).unwrap();
        let entries = build_session_entries(conversations);

        assert_eq!(entries.len(), 1);
        assert!(entries[0].summary.contains("test message"));
    }

    #[tokio::test]
    async fn test_save_and_load_file() {
        use super::ChatSubcommand;

        let mut os = Os::new().await.unwrap();
        let user_input = "who is the 16th president of US";

        let (_, mut session) =
            create_test_session(&mut os, vec![user_input, "exit"], vec!["Abraham Lincoln"], None).await;

        let path_str = "test_session.json".to_string();

        ChatSubcommand::Save {
            path: path_str.clone(),
            force: true,
        }
        .execute(&mut os, &mut session)
        .await
        .unwrap();

        let (_, mut session2) = create_test_session(&mut os, vec![], vec![], None).await;

        ChatSubcommand::Load { path: path_str }
            .execute(&mut os, &mut session2)
            .await
            .unwrap();

        assert_eq!(
            session.conversation.history().len(),
            session2.conversation.history().len(),
            "Loaded session should have same history length"
        );
    }
}
