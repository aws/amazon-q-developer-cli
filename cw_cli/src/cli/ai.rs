use std::fmt::Display;
use std::io::stdout;

use arboard::Clipboard;
use clap::Args;
use color_eyre::owo_colors::OwoColorize;
use crossterm::style::Stylize;
use dialoguer::theme::ColorfulTheme;
use fig_api_client::ai::{
    request_cw,
    CodewhipererFileContext,
    CodewhipererRequest,
    LanguageName,
    ProgrammingLanguage,
};
use fig_ipc::{
    BufferedUnixStream,
    SendMessage,
};
use serde::{
    Deserialize,
    Serialize,
};

use crate::util::spinner::{
    Spinner,
    SpinnerComponent,
};

const MAX_QUESTION_LEN: usize = 250;

const SEEN_ONBOARDING_KEY: &str = "ai.seen-onboarding";

#[derive(Debug, Args, PartialEq, Eq)]
pub struct AiArgs {
    input: Vec<String>,
    /// Number of completions to generate (must be <=5)
    #[arg(short, long, hide = true)]
    n: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Choice {
    text: Option<String>,
    additional_message: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CompleteResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Clone)]
enum DialogActions {
    Execute {
        command: String,
        display: bool,
    },
    Edit {
        command: String,
        display: bool,
    },
    #[allow(dead_code)]
    Copy {
        command: String,
        display: bool,
    },
    Regenerate,
    Ask,
    Cancel,
}

impl Display for DialogActions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DialogActions::Execute { command, display } => {
                if *display {
                    write!(f, "⚡ Execute {}", command.bright_magenta())
                } else {
                    write!(f, "⚡ Execute command")
                }
            },
            DialogActions::Edit { command, display } => {
                if *display {
                    write!(f, "📝 Edit {}", command.bright_magenta())
                } else {
                    write!(f, "📝 Edit command")
                }
            },
            DialogActions::Copy { command, display } => {
                if *display {
                    write!(f, "📋 Copy {}", command.bright_magenta())
                } else {
                    write!(f, "📋 Copy to clipboard")
                }
            },
            DialogActions::Regenerate => write!(f, "🔄 Regenerate answer"),
            DialogActions::Ask => write!(f, "❓ Ask another question"),
            DialogActions::Cancel => write!(f, "❌ Cancel"),
        }
    }
}

fn theme() -> ColorfulTheme {
    ColorfulTheme {
        success_prefix: dialoguer::console::style(" ".into()),
        values_style: dialoguer::console::Style::new().magenta().bright(),
        ..crate::util::dialoguer_theme()
    }
}

async fn send_figterm(text: String, execute: bool) -> eyre::Result<()> {
    let session_id = std::env::var("CWTERM_SESSION_ID")?;
    let mut conn = BufferedUnixStream::connect(fig_util::directories::figterm_socket_path(&session_id)?).await?;
    conn.send_message(fig_proto::figterm::FigtermRequestMessage {
        request: Some(fig_proto::figterm::figterm_request_message::Request::InsertOnNewCmd(
            fig_proto::figterm::InsertOnNewCmdRequest {
                text,
                execute,
                bracketed: true,
            },
        )),
    })
    .await?;
    Ok(())
}

impl AiArgs {
    pub async fn execute(self) -> eyre::Result<()> {
        // show onboarding if it hasnt been seen
        let seen_onboarding = fig_settings::state::get_bool_or(SEEN_ONBOARDING_KEY, false);

        if !seen_onboarding {
            println!();
            println!(
                "  Translate {} to {} commands. Run in any shell.",
                "English".bold(),
                "Bash".bold()
            );
            fig_settings::state::set_value(SEEN_ONBOARDING_KEY, true).ok();
        }

        println!();

        let Self { input, n } = self;
        let mut input = if input.is_empty() { None } else { Some(input.join(" ")) };

        if n.map(|n| n > 5).unwrap_or_default() {
            eyre::bail!("n must be <= 5");
        }

        // hack to show cursor which dialoguer eats
        tokio::spawn(async {
            tokio::signal::ctrl_c().await.unwrap();
            crossterm::execute!(stdout(), crossterm::cursor::Show).unwrap();
            std::process::exit(0);
        });

        'ask_loop: loop {
            let question = match input {
                Some(ref input) => input.clone(),
                None => {
                    println!("{}", "Translate Text to Bash".bold());
                    println!();

                    dialoguer::Input::with_theme(&theme())
                        .with_prompt("Text")
                        .validate_with(|input: &String| -> Result<(), String> {
                            if input.trim().len() > MAX_QUESTION_LEN {
                                Err(format!("Input is >{MAX_QUESTION_LEN} characters"))
                            } else {
                                Ok(())
                            }
                        })
                        .interact_text()?
                },
            };

            let question = question.trim().replace('\n', " ");

            if question.len() > MAX_QUESTION_LEN {
                eyre::bail!("input is >{MAX_QUESTION_LEN} characters");
            }

            'generate_loop: loop {
                let spinner_text = format!("  {} {} ", "Bash".bold(), "·".grey());

                let mut spinner = Spinner::new(vec![
                    SpinnerComponent::Text(spinner_text.clone()),
                    SpinnerComponent::Spinner,
                ]);

                let response = request_cw(CodewhipererRequest {
                    file_context: CodewhipererFileContext {
                        left_file_content: format!(
                            "# List files
ls -l

# Count files in a directory
ls -l | wc -l

# Disk space used by home directory
du ~

# Replace foo with bar in all .py files
sed 's/foo/bar/g' *.py

# Add all files to git and create a commit with the message \"feat: add new route\"
git add -A && git commit -m 'feat: add new route'

# Delete the models subdirectory
rm -rf ./models

# What folder am I in?
pwd

# {question}\n"
                        ),
                        right_file_content: "".into(),
                        filename: "commands.sh".into(),
                        programming_language: ProgrammingLanguage {
                            language_name: LanguageName::Shell,
                        },
                    },
                    max_results: 1,
                    next_token: None,
                })
                .await?;

                let choices: Vec<(String, Option<String>)> = response
                    .completions
                    .unwrap_or_default()
                    .into_iter()
                    .map(|rec| (rec.content.unwrap_or_default(), None))
                    .collect();

                macro_rules! handle_action {
                    ($action:expr) => {
                        let accepted = matches!(&$action, &Some(DialogActions::Execute { .. }));
                        fig_telemetry::send_translation_actioned(accepted).await;

                        match $action {
                            Some(DialogActions::Execute { command, .. }) => {
                                if send_figterm(command.to_owned(), true).await.is_err() {
                                    let mut child = tokio::process::Command::new("bash")
                                        .arg("-c")
                                        .arg(command)
                                        .spawn()?;
                                    child.wait().await?;
                                }
                                break 'ask_loop;
                            },
                            Some(DialogActions::Edit { command, .. }) => {
                                if let Err(err) = send_figterm(command.to_owned(), false).await {
                                    println!("{} {err}", "Failed to insert command:".bright_red().bold());
                                    println!();
                                    println!("Command: {command}");
                                }
                                break 'ask_loop;
                            },
                            Some(DialogActions::Copy { command, .. }) => {
                                if let Ok(mut clipboard) = Clipboard::new() {
                                    match clipboard.set_text(command.to_string()) {
                                        Ok(_) => println!("Copied!"),
                                        Err(err) => eyre::bail!(err),
                                    }
                                }
                                break 'ask_loop;
                            },
                            Some(DialogActions::Regenerate) => {
                                continue 'generate_loop;
                            },
                            Some(DialogActions::Ask) => {
                                input = None;
                                continue 'ask_loop;
                            },
                            _ => break 'ask_loop,
                        }
                    };
                }

                match &choices[..] {
                    [] => {
                        spinner.stop_with_message(format!("{spinner_text}❌"));
                        eyre::bail!("no valid completions were generated");
                    },
                    [(choice, additional_message)] => {
                        spinner.stop_with_message(format!("{spinner_text}{}", choice.bright_magenta()));
                        if let Some(additional_message) = additional_message {
                            println!("  {additional_message}");
                        }
                        println!();

                        let actions: Vec<DialogActions> = fig_settings::settings::get("ai.menu-actions")
                            .ok()
                            .flatten()
                            .unwrap_or_else(|| {
                                ["execute", "edit", "regenerate", "ask", "cancel"]
                                    .map(String::from)
                                    .to_vec()
                            })
                            .into_iter()
                            .filter_map(|action| match action.as_str() {
                                "execute" => Some(DialogActions::Execute {
                                    command: choice.to_string(),
                                    display: false,
                                }),
                                "edit" => Some(DialogActions::Edit {
                                    command: choice.to_string(),
                                    display: false,
                                }),
                                "copy" => Some(DialogActions::Copy {
                                    command: choice.to_string(),
                                    display: false,
                                }),
                                "regenerate" => Some(DialogActions::Regenerate),
                                "ask" => Some(DialogActions::Ask),
                                "cancel" => Some(DialogActions::Cancel),
                                _ => None,
                            })
                            .collect();

                        let selected = dialoguer::Select::with_theme(&crate::util::dialoguer_theme())
                            .default(0)
                            .items(&actions)
                            .interact_opt()?;

                        handle_action!(selected.and_then(|i| actions.get(i)));
                    },
                    choices => {
                        spinner.stop_with_message(format!("{spinner_text}{}", "<multiple options>".dark_grey()));
                        println!();

                        let mut actions: Vec<_> = choices
                            .iter()
                            .map(|(choice, _)| DialogActions::Execute {
                                command: choice.to_string(),
                                display: true,
                            })
                            .collect();

                        actions.extend_from_slice(&[
                            DialogActions::Regenerate,
                            DialogActions::Ask,
                            DialogActions::Cancel,
                        ]);

                        let selected = dialoguer::Select::with_theme(&crate::util::dialoguer_theme())
                            .default(0)
                            .items(&actions)
                            .interact_opt()?;

                        handle_action!(selected.and_then(|i| actions.get(i)));
                    },
                }
            }
        }

        Ok(())
    }
}