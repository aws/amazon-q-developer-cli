use std::env;
use std::path::Path;

use amzn_codewhisperer_streaming_client::operation::RequestId;
use aws_smithy_types::error::display::DisplayErrorContext;
use eyre::{
    Result,
    bail,
};
use fig_api_client::StreamingClient;
use fig_api_client::model::{
    ChatResponseStream,
    ConversationState,
    EnvState,
    EnvironmentVariable,
    GitState,
    ShellHistoryEntry,
    ShellState,
    UserInputMessage,
    UserInputMessageContext,
    UserIntent,
};
use fig_settings::history::{
    History,
    OrderBy,
};
use fig_util::Shell;
use once_cell::sync::Lazy;
use regex::Regex;
use tokio::sync::mpsc::{
    UnboundedReceiver,
    UnboundedSender,
};
use tracing::error;

use super::ApiResponse;

// Max constants for length of strings and lists, use these to truncate elements
// to ensure the API request is valid

// These limits are the internal undocumented values from the service for each item

const MAX_ENV_VAR_LIST_LEN: usize = 100;
const MAX_ENV_VAR_KEY_LEN: usize = 256;
const MAX_ENV_VAR_VALUE_LEN: usize = 1024;
const MAX_CURRENT_WORKING_DIRECTORY_LEN: usize = 256;

const MAX_GIT_STATUS_LEN: usize = 4096;

const MAX_SHELL_HISTORY_LIST_LEN: usize = 20;
const MAX_SHELL_HISTORY_COMMAND_LEN: usize = 1024;
const MAX_SHELL_HISTORY_DIRECTORY_LEN: usize = 256;

/// Regex for the context modifiers `@git`, `@env`, and `@history`
static CONTEXT_MODIFIER_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"@(git|env|history) ?").unwrap());

fn truncate_safe(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }

    let mut byte_count = 0;
    let mut char_indices = s.char_indices();

    for (byte_idx, _) in &mut char_indices {
        if byte_count + (byte_idx - byte_count) > max_bytes {
            break;
        }
        byte_count = byte_idx;
    }

    &s[..byte_count]
}

/// The context modifiers that are used in a specific chat message
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ContextModifiers {
    env: bool,
    history: bool,
    git: bool,
}

impl ContextModifiers {
    /// Returns `true` if any context modifiers are set
    fn any(&self) -> bool {
        self.env || self.history || self.git
    }

    /// Returns a [`UserIntent`] that disables RAG if any context modifiers are set
    #[allow(clippy::unused_self)]
    fn user_intent(&self) -> Option<UserIntent> {
        // disabled while user intents all change prompt
        // if self.any() {
        //     Some(UserIntent::ApplyCommonBestPractices)
        // } else {
        //     None
        // }
        None
    }
}

/// Convert the `input` into the [ContextModifiers] and a string with them removed
fn input_to_modifiers(input: String) -> (ContextModifiers, String) {
    let mut modifiers = ContextModifiers::default();

    for capture in CONTEXT_MODIFIER_REGEX.captures_iter(&input) {
        let modifier = capture.get(1).expect("regex has a captrue group").as_str();

        match modifier {
            "git" => modifiers.git = true,
            "env" => modifiers.env = true,
            "history" => modifiers.history = true,
            _ => unreachable!(),
        }
    }

    (modifiers, input)
}

fn build_shell_history(history: &History) -> Option<Vec<ShellHistoryEntry>> {
    let mut shell_history = vec![];

    if let Ok(commands) = history.rows(
        None,
        vec![OrderBy::new(
            fig_settings::history::HistoryColumn::Id,
            fig_settings::history::Order::Desc,
        )],
        MAX_SHELL_HISTORY_LIST_LEN,
        0,
    ) {
        for command in commands.into_iter().filter(|c| c.command.is_some()).rev() {
            let command_str = command.command.expect("command is filtered on");
            if !command_str.is_empty() {
                shell_history.push(ShellHistoryEntry {
                    command: truncate_safe(&command_str, MAX_SHELL_HISTORY_COMMAND_LEN).into(),
                    directory: command.cwd.and_then(|cwd| {
                        if !cwd.is_empty() {
                            Some(truncate_safe(&cwd, MAX_SHELL_HISTORY_DIRECTORY_LEN).into())
                        } else {
                            None
                        }
                    }),
                    exit_code: command.exit_code,
                });
            }
        }
    }

    if shell_history.is_empty() {
        None
    } else {
        Some(shell_history)
    }
}

fn build_shell_state(shell_history: bool, history: &History) -> ShellState {
    // Try to grab the shell from the parent process via the `Shell::current_shell`,
    // then try the `SHELL` env, finally just report bash
    let shell_name = Shell::current_shell()
        .or_else(|| {
            let shell_name = env::var("SHELL").ok()?;
            Shell::try_find_shell(shell_name)
        })
        .unwrap_or(Shell::Bash)
        .to_string();

    let mut shell_state = ShellState {
        shell_name,
        shell_history: None,
    };

    if shell_history {
        shell_state.shell_history = build_shell_history(history);
    }

    shell_state
}

fn build_env_state(modifiers: &ContextModifiers) -> EnvState {
    let mut env_state = EnvState {
        operating_system: Some(env::consts::OS.into()),
        ..Default::default()
    };

    if modifiers.any() {
        if let Ok(current_dir) = env::current_dir() {
            env_state.current_working_directory =
                Some(truncate_safe(&current_dir.to_string_lossy(), MAX_CURRENT_WORKING_DIRECTORY_LEN).into());
        }
    }

    if modifiers.env {
        for (key, value) in env::vars().take(MAX_ENV_VAR_LIST_LEN) {
            if !key.is_empty() && !value.is_empty() {
                env_state.environment_variables.push(EnvironmentVariable {
                    key: truncate_safe(&key, MAX_ENV_VAR_KEY_LEN).into(),
                    value: truncate_safe(&value, MAX_ENV_VAR_VALUE_LEN).into(),
                });
            }
        }
    }

    env_state
}

async fn build_git_state(dir: Option<&Path>) -> Result<GitState> {
    // git status --porcelain=v1 -b
    let mut command = tokio::process::Command::new("git");
    command.args(["status", "--porcelain=v1", "-b"]);
    if let Some(dir) = dir {
        command.current_dir(dir);
    }
    let output = command.output().await?;

    if output.status.success() && !output.stdout.is_empty() {
        Ok(GitState {
            status: truncate_safe(&String::from_utf8_lossy(&output.stdout), MAX_GIT_STATUS_LEN).into(),
        })
    } else {
        bail!("git status failed: {output:?}")
    }
}

async fn try_send_message(
    client: StreamingClient,
    tx: &UnboundedSender<ApiResponse>,
    conversation_state: ConversationState,
) -> Result<(), String> {
    let mut send_message_output = match client.send_message(conversation_state).await {
        Ok(output) => output,
        Err(err) => {
            if err.is_service_error() {
                error!(err =% DisplayErrorContext(&err), "send_message failed");
                tx.send(ApiResponse::Error(Some(err.to_string())))
                    .map_err(|err| format!("tx failed to send ApiResponse::Error: {err}"))?;
                return Ok(());
            } else {
                return Err(format!("send_message failed: {}", DisplayErrorContext(err)));
            }
        },
    };

    if let Some(message_id) = send_message_output.request_id().map(ToOwned::to_owned) {
        tx.send(ApiResponse::MessageId(message_id))
            .map_err(|err| format!("tx failed to send ApiResponse::MessageId: {err}"))?;
    }

    loop {
        #[allow(clippy::collapsible_match)]
        match send_message_output.recv().await {
            Ok(Some(stream)) => match stream {
                ChatResponseStream::AssistantResponseEvent { content } => {
                    tx.send(ApiResponse::Text(content))
                        .map_err(|err| format!("tx failed to send ApiResponse::Text: {err}"))?;
                },
                #[allow(clippy::collapsible_match)]
                ChatResponseStream::MessageMetadataEvent { conversation_id, .. } => {
                    if let Some(id) = conversation_id {
                        tx.send(ApiResponse::ConversationId(id))
                            .map_err(|err| format!("tx failed to send ApiResponse::ConversationId: {err}"))?;
                    }
                },
                ChatResponseStream::FollowupPromptEvent(_event) => {},
                ChatResponseStream::CodeReferenceEvent(_event) => {},
                ChatResponseStream::SupplementaryWebLinksEvent(_event) => {},
                ChatResponseStream::InvalidStateEvent { reason, message } => {
                    error!(%reason, %message, "invalid state event");
                },
                _ => {},
            },
            Ok(None) => break,
            Err(err) => {
                return Err(format!("send_message_output.recv failed: {}", DisplayErrorContext(err)));
            },
        }
    }

    Ok(())
}

pub(super) async fn send_message(
    client: StreamingClient,
    input: String,
    conversation_id: Option<String>,
) -> Result<UnboundedReceiver<ApiResponse>> {
    let (ctx, input) = input_to_modifiers(input);

    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

    let history = History::new();

    let mut user_input_message_context = UserInputMessageContext {
        shell_state: Some(build_shell_state(ctx.history, &history)),
        env_state: Some(build_env_state(&ctx)),
        ..Default::default()
    };

    if ctx.git {
        if let Ok(git_state) = build_git_state(None).await {
            user_input_message_context.git_state = Some(git_state);
        }
    }

    let user_input_message = UserInputMessage {
        content: input,
        user_input_message_context: Some(user_input_message_context),
        user_intent: ctx.user_intent(),
    };

    let conversation_state = ConversationState {
        conversation_id,
        user_input_message,
        history: None,
    };

    tokio::spawn(async move {
        if let Err(err) = try_send_message(client, &tx, conversation_state).await {
            error!(%err, "try_send_message failed");
            if let Err(err) = tx.send(ApiResponse::Error(None)) {
                error!(%err, "tx failed to send ApiResponse::Error");
            };
            return;
        }

        // Try to end stream
        tx.send(ApiResponse::End).ok();
    });

    Ok(rx)
}

#[cfg(test)]
mod tests {
    use fig_api_client::StreamingClient;
    use fig_settings::history::CommandInfo;
    use tokio::io::AsyncWriteExt;

    use super::*;

    #[test]
    fn test_truncate_safe() {
        assert_eq!(truncate_safe("Hello World", 5), "Hello");
        assert_eq!(truncate_safe("Hello ", 5), "Hello");
        assert_eq!(truncate_safe("Hello World", 11), "Hello World");
        assert_eq!(truncate_safe("Hello World", 15), "Hello World");
    }

    #[tokio::test]
    #[ignore = "not in ci"]
    async fn test_send_message() {
        let client = StreamingClient::new().await.unwrap();
        let question = "@git Explain my git status.".to_string();

        let mut rx = send_message(client.clone(), question, None).await.unwrap();

        while let Some(res) = rx.recv().await {
            match res {
                ApiResponse::Text(text) => {
                    let mut stderr = tokio::io::stderr();
                    stderr.write_all(text.as_bytes()).await.unwrap();
                    stderr.flush().await.unwrap();
                },
                ApiResponse::ConversationId(_) => (),
                ApiResponse::MessageId(_) => (),
                ApiResponse::Error(err) => panic!("{err:?}"),
                ApiResponse::End => break,
            }
        }
    }

    #[tokio::test]
    async fn test_try_send_message() {
        let client = StreamingClient::mock(vec![
            ChatResponseStream::MessageMetadataEvent {
                conversation_id: Some("abc".into()),
                utterance_id: None,
            },
            ChatResponseStream::assistant_response("Hello World"),
            ChatResponseStream::FollowupPromptEvent(()),
            ChatResponseStream::CodeReferenceEvent(()),
            ChatResponseStream::SupplementaryWebLinksEvent(()),
            ChatResponseStream::InvalidStateEvent {
                reason: "reason".into(),
                message: "message".into(),
            },
        ]);

        let mut rx = send_message(client.clone(), "Question".into(), None).await.unwrap();

        while let Some(res) = rx.recv().await {
            match res {
                ApiResponse::Text(text) => {
                    println!("{text}");
                },
                ApiResponse::ConversationId(_) => (),
                ApiResponse::MessageId(_) => (),
                ApiResponse::Error(err) => panic!("{err:?}"),
                ApiResponse::End => break,
            }
        }
    }

    #[test]
    fn test_input_to_modifiers() {
        let (modifiers, input) = input_to_modifiers("How do I use git?".to_string());
        assert_eq!(modifiers, ContextModifiers::default());
        assert_eq!(input, "How do I use git?");

        let (modifiers, input) = input_to_modifiers("@git @env @history How do I use git?".to_string());
        assert_eq!(modifiers, ContextModifiers {
            env: true,
            history: true,
            git: true
        });
        assert_eq!(input, "@git @env @history How do I use git?");

        let (modifiers, input) = input_to_modifiers("@git How do I use git?".to_string());
        assert_eq!(modifiers, ContextModifiers {
            env: false,
            history: false,
            git: true
        });
        assert_eq!(input, "@git How do I use git?");

        let (modifiers, input) = input_to_modifiers("@env How do I use git?".to_string());
        assert_eq!(modifiers, ContextModifiers {
            env: true,
            history: false,
            git: false
        });
        assert_eq!(input, "@env How do I use git?");
    }

    #[test]
    fn test_shell_state() {
        let history = History::mock();
        history
            .insert_command_history(
                &CommandInfo {
                    command: Some("ls".into()),
                    cwd: Some("/home/user".into()),
                    exit_code: Some(0),
                    ..Default::default()
                },
                false,
            )
            .unwrap();

        let shell_state = build_shell_state(true, &history);

        for ShellHistoryEntry {
            command,
            directory,
            exit_code,
        } in shell_state.shell_history.unwrap()
        {
            println!("{command} {directory:?} {exit_code:?}");
        }
    }

    #[test]
    fn test_env_state() {
        // env: true
        let env_state = build_env_state(&ContextModifiers {
            env: true,
            history: false,
            git: false,
        });
        assert!(!env_state.environment_variables.is_empty());
        assert!(!env_state.current_working_directory.as_ref().unwrap().is_empty());
        assert!(!env_state.operating_system.as_ref().unwrap().is_empty());
        println!("{env_state:?}");

        // env: false
        let env_state = build_env_state(&ContextModifiers::default());
        assert!(env_state.environment_variables.is_empty());
        assert!(env_state.current_working_directory.is_none());
        assert!(!env_state.operating_system.as_ref().unwrap().is_empty());
        println!("{env_state:?}");
    }

    async fn init_git_repo(dir: &Path) {
        let output = tokio::process::Command::new("git")
            .arg("init")
            .current_dir(dir)
            .output()
            .await
            .unwrap();
        println!("== git init ==");
        println!("stdout: {}", String::from_utf8_lossy(&output.stdout));
        println!("stderr: {}", String::from_utf8_lossy(&output.stderr));
        assert!(output.status.success());
    }

    #[tokio::test]
    async fn test_git_state() {
        let tempdir = tempfile::tempdir().unwrap();
        let dir_path = tempdir.path();

        // write a file to the repo to ensure git status has a change
        let path = dir_path.join("test.txt");
        std::fs::write(path, "test").unwrap();

        let git_state_err = build_git_state(Some(dir_path)).await.unwrap_err();
        println!("git_state_err: {git_state_err}");
        assert!(git_state_err.to_string().contains("git status failed"));

        init_git_repo(dir_path).await;

        let git_state = build_git_state(Some(dir_path)).await.unwrap();
        println!("git_state: {git_state:?}");
    }
}
