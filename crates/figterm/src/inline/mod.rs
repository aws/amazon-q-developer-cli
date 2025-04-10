mod completion_cache;
mod validate;

use std::fmt::Write;
use std::sync::LazyLock;
use std::time::{
    Duration,
    Instant,
    SystemTime,
};

use fig_api_client::Client;
use fig_api_client::clients::FILE_CONTEXT_LEFT_FILE_CONTENT_MAX_LEN;
use fig_api_client::model::{
    FileContext,
    LanguageName,
    ProgrammingLanguage,
    RecommendationsInput,
};
use fig_proto::figterm::figterm_response_message::Response as FigtermResponse;
use fig_proto::figterm::{
    FigtermResponseMessage,
    InlineShellCompletionAcceptRequest,
    InlineShellCompletionRequest,
    InlineShellCompletionResponse,
    InlineShellCompletionSetEnabledRequest,
};
use fig_settings::history::CommandInfo;
use fig_telemetry::{
    AppTelemetryEvent,
    SuggestionState,
};
use fig_util::Shell;
use fig_util::terminal::{
    current_terminal,
    current_terminal_version,
};
use flume::Sender;
use regex::Regex;
use tokio::sync::Mutex;
use tracing::{
    error,
    info,
    warn,
};
use validate::validate;

use self::completion_cache::CompletionCache;
use crate::history::{
    self,
    HistoryQueryParams,
    HistorySender,
};

const HISTORY_COUNT_DEFAULT: usize = 49;
const DEBOUNCE_DURATION_DEFAULT: Duration = Duration::from_millis(300);

static INLINE_ENABLED: Mutex<bool> = Mutex::const_new(true);

static LAST_RECEIVED: Mutex<Option<SystemTime>> = Mutex::const_new(None);

static CACHE_ENABLED: LazyLock<bool> =
    LazyLock::new(|| std::env::var_os("Q_INLINE_SHELL_COMPLETION_CACHE_DISABLE").is_none());
static COMPLETION_CACHE: LazyLock<Mutex<CompletionCache>> = LazyLock::new(|| Mutex::new(CompletionCache::new()));

static TELEMETRY_QUEUE: Mutex<TelemetryQueue> = Mutex::const_new(TelemetryQueue::new());

static HISTORY_COUNT: LazyLock<usize> = LazyLock::new(|| {
    std::env::var("Q_INLINE_SHELL_COMPLETION_HISTORY_COUNT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(HISTORY_COUNT_DEFAULT)
});
static DEBOUNCE_DURATION: LazyLock<Duration> = LazyLock::new(|| {
    std::env::var("Q_INLINE_SHELL_COMPLETION_DEBOUNCE_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .map_or(DEBOUNCE_DURATION_DEFAULT, Duration::from_millis)
});

pub async fn on_prompt() {
    COMPLETION_CACHE.lock().await.clear();
    TELEMETRY_QUEUE.lock().await.send_all_items(None).await;
}

struct TelemetryQueue {
    items: Vec<TelemetryQueueItem>,
}

impl TelemetryQueue {
    const fn new() -> Self {
        Self { items: Vec::new() }
    }

    async fn send_all_items(&mut self, retain: Option<usize>) {
        let start_url = fig_auth::builder_id_token()
            .await
            .ok()
            .flatten()
            .and_then(|t| t.start_url);

        let items_len = self.items.len();
        let drain_len = retain.map_or(items_len, |n| items_len.saturating_sub(n));

        for item in self.items.drain(..drain_len) {
            let TelemetryQueueItem {
                timestamp,
                session_id,
                request_id,
                suggestion_state,
                edit_buffer_len,
                suggested_chars_len,
                number_of_recommendations,
                latency,
                ..
            } = item;

            fig_telemetry::send_event(
                AppTelemetryEvent::from_event(fig_telemetry_core::Event {
                    created_time: Some(timestamp),
                    credential_start_url: start_url.clone(),
                    ty: fig_telemetry::EventType::InlineShellCompletionActioned {
                        session_id,
                        request_id,
                        suggestion_state,
                        edit_buffer_len,
                        suggested_chars_len,
                        number_of_recommendations,
                        latency,
                        terminal: current_terminal().map(|s| s.internal_id().into_owned()),
                        terminal_version: current_terminal_version().map(Into::into),
                        // The only supported shell currently is Zsh
                        shell: Some(Shell::Zsh.as_str().into()),
                        shell_version: None,
                    },
                })
                .await,
            )
            .await;

            // prevent more than 2 events per second
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }
}

struct TelemetryQueueItem {
    buffer: String,
    suggestion: String,

    timestamp: SystemTime,

    session_id: String,
    request_id: String,
    suggestion_state: SuggestionState,
    edit_buffer_len: Option<i64>,
    suggested_chars_len: i32,
    number_of_recommendations: i32,
    latency: Duration,
}

pub async fn handle_request(
    figterm_request: InlineShellCompletionRequest,
    _session_id: String,
    response_tx: Sender<FigtermResponseMessage>,
    history_sender: HistorySender,
) {
    if !*INLINE_ENABLED.lock().await {
        return;
    }

    let buffer = figterm_request.buffer.trim_start();

    if *CACHE_ENABLED {
        // use cached completion if available
        if let Some(insert_text) = COMPLETION_CACHE.lock().await.get_insert_text(buffer) {
            let trimmed_insert = insert_text.strip_prefix(buffer).unwrap_or(insert_text);

            if let Err(err) = response_tx
                .send_async(FigtermResponseMessage {
                    response: Some(FigtermResponse::InlineShellCompletion(InlineShellCompletionResponse {
                        insert_text: Some(trimmed_insert.to_owned()),
                    })),
                })
                .await
            {
                error!(%err, "Failed to send inline_shell_completion completion");
            }
            return;
        }
    }

    // debounce requests
    let now = SystemTime::now();
    LAST_RECEIVED.lock().await.replace(now);

    let Ok(client) = Client::new().await else {
        return;
    };

    for _ in 0..3 {
        tokio::time::sleep(*DEBOUNCE_DURATION).await;
        if *LAST_RECEIVED.lock().await == Some(now) {
            // TODO: determine behavior here, None or Some(unix timestamp)
            *LAST_RECEIVED.lock().await = Some(SystemTime::now());
        } else {
            warn!("Received another inline_shell_completion completion request, aborting");
            if let Err(err) = response_tx
                .send_async(FigtermResponseMessage {
                    response: Some(FigtermResponse::InlineShellCompletion(InlineShellCompletionResponse {
                        insert_text: None,
                    })),
                })
                .await
            {
                error!(%err, "Failed to send inline_shell_completion completion");
            }

            return;
        }

        info!("Sending inline_shell_completion completion request");

        let (history_query_tx, history_query_rx) = flume::bounded(1);
        if let Err(err) = history_sender
            .send_async(history::HistoryCommand::Query(
                HistoryQueryParams { limit: *HISTORY_COUNT },
                history_query_tx,
            ))
            .await
        {
            error!(%err, "Failed to send history query");
        }

        let history = match history_query_rx.recv_async().await {
            Ok(Some(history)) => history,
            err => {
                error!(?err, "Failed to get history");
                vec![]
            },
        };

        let Some(prompt) = prompt(&history, buffer) else {
            return;
        };

        let input = RecommendationsInput {
            file_context: FileContext {
                left_file_content: prompt,
                right_file_content: "".into(),
                filename: "history.sh".into(),
                programming_language: ProgrammingLanguage {
                    language_name: LanguageName::Shell,
                },
            },
            max_results: 1,
            next_token: None,
        };

        let start_instant = Instant::now();

        let response = match client.generate_recommendations(input).await {
            Err(err) if err.is_throttling_error() => {
                warn!(%err, "Too many requests, trying again in 1 second");
                tokio::time::sleep(Duration::from_secs(1).saturating_sub(*DEBOUNCE_DURATION)).await;
                continue;
            },
            other => other,
        };

        let insert_text = match response {
            Ok(output) => {
                let request_id = output.request_id.unwrap_or_default();
                let session_id = output.session_id.unwrap_or_default();
                let recommendations = output.recommendations;
                let number_of_recommendations = recommendations.len() as i32;
                let mut completion_cache = COMPLETION_CACHE.lock().await;

                let mut completions = recommendations
                    .into_iter()
                    .map(|choice| clean_completion(&choice.content).clone())
                    .collect::<Vec<_>>();

                // skips the first one which we will recommend, we only cache the rest
                for completion in completions.iter().skip(1) {
                    let full_text = format!("{buffer}{completion}");
                    if !completion.is_empty() && validate(&full_text) {
                        completion_cache.insert(full_text, 1.0);
                    }
                }

                // now deals with the first recommendation
                if let Some(completion) = completions.first_mut() {
                    let full_text = format!("{buffer}{completion}");
                    let valid = validate(&full_text);
                    let is_empty = completion.is_empty();

                    if valid && !is_empty {
                        completion_cache.insert(full_text, 0.0);
                    }

                    let suggestion_state = match (valid, completion.is_empty()) {
                        (true, true) => SuggestionState::Empty,
                        (true, false) => SuggestionState::Accept,
                        (false, _) => SuggestionState::Discard,
                    };

                    tokio::spawn({
                        let completion = completion.clone();
                        let buffer = buffer.to_owned();
                        async move {
                            let mut queue = TELEMETRY_QUEUE.lock().await;
                            queue.items.push(TelemetryQueueItem {
                                suggested_chars_len: completion.chars().count() as i32,
                                number_of_recommendations,
                                suggestion: completion,
                                timestamp: SystemTime::now(),
                                session_id,
                                request_id,
                                latency: start_instant.elapsed(),
                                suggestion_state,
                                edit_buffer_len: buffer.chars().count().try_into().ok(),
                                buffer,
                            });
                            // flush all but 4 messages, this is to retain messages that might have
                            // an accept waiting
                            queue.send_all_items(Some(4)).await;
                        }
                    });

                    if valid { Some(std::mem::take(completion)) } else { None }
                } else {
                    None
                }
            },
            Err(err) => {
                error!(%err, "Failed to get inline_shell_completion completion");
                None
            },
        };

        info!(?insert_text, "Got inline_shell_completion completion");

        match response_tx
            .send_async(FigtermResponseMessage {
                response: Some(FigtermResponse::InlineShellCompletion(InlineShellCompletionResponse {
                    insert_text,
                })),
            })
            .await
        {
            Ok(()) => {},
            Err(err) => {
                // This means the user typed something else before we got a response
                // We want to bump the debounce timer

                error!(%err, "Failed to send inline_shell_completion completion");
            },
        }

        break;
    }
}

pub async fn handle_accept(figterm_request: InlineShellCompletionAcceptRequest, _session_id: String) {
    let mut queue = TELEMETRY_QUEUE.lock().await;
    for item in queue.items.iter_mut() {
        if item.buffer == figterm_request.buffer.trim_start() && item.suggestion == figterm_request.suggestion {
            item.suggestion_state = SuggestionState::Accept;
        }
    }
    queue.send_all_items(None).await;
}

pub async fn handle_set_enabled(figterm_request: InlineShellCompletionSetEnabledRequest, _session_id: String) {
    *INLINE_ENABLED.lock().await = figterm_request.enabled;
}

fn prompt(history: &[CommandInfo], buffer: &str) -> Option<String> {
    for i in (0..history.len()).rev() {
        let formatted_prompt = history
            .iter()
            .rev()
            .take(i + 1)
            .filter_map(|c| c.command.clone())
            .chain([buffer.into()])
            .enumerate()
            .fold(String::new(), |mut acc, (i, c)| {
                if i > 0 {
                    acc.push('\n');
                }
                let _ = write!(acc, "{:>5}  {c}", i + 1);
                acc
            });

        if formatted_prompt.len() < FILE_CONTEXT_LEFT_FILE_CONTENT_MAX_LEN {
            return Some(formatted_prompt);
        }
    }
    None
}

static RE_1: LazyLock<Regex> = LazyLock::new(|| Regex::new(&format!("{}\\s+.*", *HISTORY_COUNT + 1)).unwrap());
static RE_2: LazyLock<Regex> = LazyLock::new(|| Regex::new(&format!("{}\\s+.*", *HISTORY_COUNT + 2)).unwrap());

fn clean_completion(response: &str) -> String {
    // only return the first line of the response
    let first_line = match response.split_once('\n') {
        Some((left, _)) => left,
        None => response,
    };

    // replace parts of the prompt that potentially are the next lines without a newline
    let res = RE_1.replace(first_line, "");
    let res = RE_2.replace(&res, "");

    // trim any remaining whitespace
    res.trim_end().to_owned()
}

#[cfg(test)]
mod tests {
    use fig_settings::history::{
        HistoryColumn,
        Order,
        OrderBy,
        WhereExpression,
    };

    use super::*;

    #[test]
    fn test_prompt() {
        let history = vec![
            CommandInfo {
                command: Some("echo world".into()),
                ..Default::default()
            },
            CommandInfo {
                command: Some("echo hello".into()),
                ..Default::default()
            },
        ];

        let prompt = prompt(&history, "echo ").unwrap();
        println!("{prompt}");

        assert_eq!(prompt, "    1  echo hello\n    2  echo world\n    3  echo ");
    }

    #[test]
    fn test_clean_completion() {
        assert_eq!(clean_completion("echo hello"), "echo hello");
        assert_eq!(clean_completion("echo hello\necho world"), "echo hello");
        assert_eq!(clean_completion("echo hello   \necho world\n"), "echo hello");
        assert_eq!(clean_completion("echo hello     "), "echo hello");

        // Trim potential excess lines from the model
        assert_eq!(clean_completion("cd           50      ls"), "cd");
        assert_eq!(
            clean_completion("git add     50  git commit -m \"initial commit\""),
            "git add"
        );
        assert_eq!(clean_completion("cd           51      ls"), "cd");
        assert_eq!(
            clean_completion("git add     51  git commit -m \"initial commit\""),
            "git add"
        );
    }

    #[test]
    fn too_long_prompt() {
        let history = vec![CommandInfo {
            command: Some("a".repeat(FILE_CONTEXT_LEFT_FILE_CONTENT_MAX_LEN + 1)),
            ..Default::default()
        }];

        assert!(prompt(&history, "echo ").is_none());
    }

    #[ignore = "not in CI"]
    #[tokio::test]
    async fn test_inline_suggestion_prompt() {
        let history = fig_settings::history::History::new();
        let commands = history
            .rows(
                Some(WhereExpression::NotNull(HistoryColumn::ExitCode)),
                vec![OrderBy::new(HistoryColumn::Id, Order::Desc)],
                *HISTORY_COUNT,
                0,
            )
            .unwrap();
        let prompt = prompt(&commands, "cd ").unwrap();

        let client = fig_api_client::Client::new().await.unwrap();
        let out = client
            .generate_recommendations(RecommendationsInput {
                file_context: FileContext {
                    left_file_content: prompt,
                    right_file_content: "".into(),
                    filename: "history.sh".into(),
                    programming_language: ProgrammingLanguage {
                        language_name: LanguageName::Shell,
                    },
                },
                max_results: 1,
                next_token: None,
            })
            .await
            .unwrap();

        println!("out: {out:?}");
    }
}
