use std::fmt::Display;
use std::io::{
    IsTerminal,
    stdout,
};
use std::process::ExitCode;
use std::sync::LazyLock;
use std::time::Instant;

use anstream::{
    eprintln,
    println,
};
use arboard::Clipboard;
use clap::Args;
use color_eyre::owo_colors::OwoColorize;
use crossterm::style::Stylize;
use dialoguer::theme::ColorfulTheme;
use eyre::{
    Result,
    bail,
};
use fig_api_client::Client;
use fig_api_client::model::{
    FileContext,
    LanguageName,
    ProgrammingLanguage,
    RecommendationsInput,
};
use fig_ipc::{
    BufferedUnixStream,
    SendMessage,
};
use fig_telemetry::SuggestionState;
use fig_util::CLI_BINARY_NAME;
use fig_util::env_var::QTERM_SESSION_ID;
use regex::{
    Captures,
    Regex,
};
use serde::{
    Deserialize,
    Serialize,
};

use crate::util::region_check;
use crate::util::spinner::{
    Spinner,
    SpinnerComponent,
};

const SEEN_ONBOARDING_KEY: &str = "ai.seen-onboarding";

#[derive(Debug, Args, Default, PartialEq, Eq)]
pub struct TranslateArgs {
    input: Vec<String>,
    /// Number of completions to generate (must be <=5)
    #[arg(short, long, hide = true)]
    n: Option<i32>,
}

impl TranslateArgs {
    pub fn new(input: Vec<String>) -> Self {
        Self {
            input,
            ..Default::default()
        }
    }
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

async fn send_figterm(text: String, execute: bool) -> Result<()> {
    let session_id = std::env::var(QTERM_SESSION_ID)?;
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

struct CwResponse {
    completions: Vec<String>,
}

async fn generate_response(question: &str, n: i32) -> Result<CwResponse> {
    let os = match std::env::consts::OS {
        "macos" => "macOS",
        "linux" => fig_util::system_info::linux::get_os_release()
            .and_then(|a| a.name.as_deref())
            .unwrap_or("Linux"),
        "windows" => "Windows",
        other => other,
    };

    let prompt_comment = format!(
        "# A collection of {os} shell one-liners that can be run interactively, they all must only be one line and line up with the comment above them"
    );

    let prompt = r#"# list all version of node on my path
which -a node | xargs -I{} bash -c 'echo -n "{}: "; {} --version'
    
# Generate all combination (e.g. A,T,C,G)
set={A,T,C,G}; group=5; for ((i=0; i<$group; i++)); do; repetition=$set$repetition; done; bash -c "echo "$repetition""
    
# Find average of input list/file of integers
i=`wc -l $FILENAME|cut -d ' ' -f1`; cat $FILENAME| echo "scale=2;(`paste -sd+`)/"$i|bc
    
#"#;

    let mut input = RecommendationsInput {
        file_context: FileContext {
            left_file_content: format!("{prompt_comment}\n\n{prompt}{question}\n"),
            right_file_content: "".into(),
            filename: "commands.sh".into(),
            programming_language: ProgrammingLanguage {
                language_name: LanguageName::Shell,
            },
        },
        max_results: n,
        next_token: None,
    };

    let mut completions = vec![];
    let client = Client::new().await?;
    loop {
        let output = client.generate_recommendations(input.clone()).await?;
        for comp in output.recommendations {
            completions.push(comp.content.clone());
        }
        match output.next_token {
            Some(next_token) if !next_token.is_empty() => {
                input.next_token = Some(next_token);
            },
            _ => break,
        }
    }
    Ok(CwResponse { completions })
}

fn warning_message(content: &str) {
    #[allow(clippy::type_complexity)]
    let warnings: &[(Regex, fn(&Captures<'_>) -> String)] = &[
        (Regex::new(r"\bsudo\b").unwrap(), |_m| {
            "⚠️ Warning: this command contains sudo which will run the command as admin, please make sure you know what you are doing before you run this...".into()
        }),
        (
            Regex::new(r"\s+(--hard|--force|-rf|--no-preserve-root)\b").unwrap(),
            |m| {
                format!(
                    "⚠️ Warning: this command contains an irreversible flag ({}), please make sure you know what you are doing before you run this...",
                    &m[0]
                )
            },
        ),
        (Regex::new(r"(\s*\/dev\/(\w*)(\s|$))").unwrap(), |_m| {
            "⚠️ Warning: this command may override one of your disks, please make sure you know what you are doing before you run this...".into()
        }),
        (
            Regex::new(r":\s*\(\s*\)\s*\{\s*:\s*\|\s*:\s*&\s*\}\s*;\s*:").unwrap(),
            |_m| "⚠️ Warning: this command is a fork bomb".into(),
        ),
        (Regex::new(r"\bdd\b").unwrap(), |_m| {
            "⚠️ Warning: dd is a dangerous command, please make sure you know what you are doing before you run it..."
                .into()
        }),
        (Regex::new(r"\|\s*(bash|sh|zsh|fish)/").unwrap(), |m| {
            format!(
                "⚠️ Warning: piping into {} can be dangerous, please make sure you know what you are doing before you run it...",
                &m[0]
            )
        }),
        (Regex::new(r"(\bsudoedit|\bsu|\/etc\/sudoers)\b/").unwrap(), |m| {
            format!(
                "⚠️ Warning: you might be altering root/sudo files with ${}`, please make sure you know what you are doing before you run it...",
                &m[0]
            )
        }),
        (Regex::new(r"(\/dev\/(u?random)|(zero))").unwrap(), |m| {
            format!(
                "⚠️ Warning: {} can be dangerous, please make sure you know what you are doing before you run this...",
                &m[0]
            )
        }),
    ];

    for (re, warning) in warnings {
        if let Some(capture) = re.captures(content) {
            println!("{}\n", warning(&capture).yellow().bold());
        }
    }
}

static PARAM_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\$[A-Za-z][A-Za-z0-9\_\-]*)").unwrap());

fn highlighter(s: &str) -> String {
    PARAM_REGEX
        .replace_all(s, |a: &Captures<'_>| {
            let env = a[0].strip_prefix('$').unwrap();
            if std::env::var_os(env).is_some() {
                a[0].into()
            } else {
                (&a[0]).bright_magenta().to_string()
            }
        })
        .into_owned()
}

#[cfg(unix)]
fn clear_stdin() -> Result<()> {
    use std::io::Read;
    use std::os::fd::AsRawFd;

    let mut stdin = std::io::stdin().lock();
    change_blocking_fd(stdin.as_raw_fd(), false)?;
    // Raw mode is required in order to read from the terminal unbuffered.
    crossterm::terminal::enable_raw_mode()?;
    let mut buf = [0u8; 64];
    loop {
        match stdin.read(&mut buf) {
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                break;
            },
            _ => (),
        }
    }
    crossterm::terminal::disable_raw_mode()?;
    change_blocking_fd(stdin.as_raw_fd(), true)?;
    Ok(())
}

#[cfg(unix)]
fn change_blocking_fd(fd: std::os::unix::io::RawFd, is_blocking: bool) -> Result<()> {
    use nix::fcntl::*;

    let flags = fcntl(fd, FcntlArg::F_GETFL)?;
    let flags = OFlag::from_bits_truncate(flags);
    fcntl(
        fd,
        FcntlArg::F_SETFL(if is_blocking {
            flags & !OFlag::O_NONBLOCK
        } else {
            flags | OFlag::O_NONBLOCK
        }),
    )?;
    Ok(())
}

impl TranslateArgs {
    pub async fn execute(self) -> Result<ExitCode> {
        if !fig_util::system_info::in_cloudshell() && !fig_auth::is_logged_in().await {
            bail!(
                "You are not logged in. Run {} to login.",
                format!("{CLI_BINARY_NAME} login").magenta()
            )
        }

        region_check("translate")?;

        let interactive = std::io::stdin().is_terminal();

        // show onboarding if it hasnt been seen
        let seen_onboarding = fig_settings::state::get_bool_or(SEEN_ONBOARDING_KEY, false);
        if !seen_onboarding && interactive {
            eprintln!();
            eprintln!(
                "  Translate {} to {} commands. Run in any shell.",
                "English".bold(),
                "Shell".bold()
            );
            fig_settings::state::set_value(SEEN_ONBOARDING_KEY, true).ok();
        }
        if interactive {
            eprintln!();
        }

        let Self { input, n } = self;
        let mut input = if input.is_empty() { None } else { Some(input.join(" ")) };

        let n = match n {
            Some(n) if n >= 0 || n > 5 => {
                eyre::bail!("n must be 0 < n <= 5");
            },
            Some(n) => n,
            None => 1,
        };

        if !interactive {
            let question = match input {
                Some(_) => {
                    eyre::bail!("only input on stdin is supported when stdin is not a tty")
                },
                None => {
                    let stdin = std::io::stdin();
                    let mut question = String::new();
                    stdin.read_line(&mut question)?;
                    question
                },
            };

            match &generate_response(&question, 1).await?.completions[..] {
                [] => eyre::bail!("no valid completions were generated"),
                [res, ..] => {
                    println!("{res}");
                    return Ok(ExitCode::SUCCESS);
                },
            };
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
                    println!("{}", "Translate Text to Shell".bold());
                    println!();

                    dialoguer::Input::with_theme(&theme())
                        .with_prompt("Text")
                        .interact_text()?
                },
            };

            let question = question.trim().replace('\n', " ");

            'generate_loop: loop {
                let spinner_text = format!("  {} {} ", "Shell".bold(), "·".grey());

                let mut spinner = Spinner::new(vec![
                    SpinnerComponent::Text(spinner_text.clone()),
                    SpinnerComponent::Spinner,
                ]);

                let response_time_start = Instant::now();
                let res = match generate_response(&question, n).await {
                    Ok(res) => res,
                    Err(err) => {
                        spinner.stop_with_message("".into());
                        return Err(err);
                    },
                };
                let response_latency = response_time_start.elapsed();

                // Prevents any buffered "enter" or "space" key presses from
                // automatically executing the command.
                #[cfg(unix)]
                clear_stdin()?;

                match &res.completions[..] {
                    [] => {
                        spinner.stop_with_message(format!("{spinner_text}❌"));
                        eyre::bail!("no valid completions were generated");
                    },
                    [choice, ..] => {
                        if let Some(error_reason) = choice.strip_prefix("# UNIMPLEMENTED: ") {
                            spinner.stop_with_message(format!("{spinner_text}❌"));
                            eyre::bail!("{}", error_reason);
                        }

                        spinner.stop_with_message(format!("{spinner_text}{}", highlighter(choice)));
                        println!();
                        warning_message(choice);

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

                        let action = selected.and_then(|i| actions.get(i));

                        fig_telemetry::send_translation_actioned(response_latency, match action {
                            Some(DialogActions::Execute { .. }) => SuggestionState::Accept,
                            _ => SuggestionState::Reject,
                        })
                        .await;

                        match action {
                            Some(DialogActions::Execute { command, .. }) => {
                                // let command = PARAM_REGEX
                                //     .replace_all(command, |a: &Captures<'_>| {
                                //         let env = a[0].strip_prefix("$").unwrap();
                                //         if std::env::var_os(env).is_some() {
                                //             a[0].to_string()
                                //         } else {
                                //             dialoguer::Input::with_theme(&theme())
                                //                 .with_prompt(env)
                                //                 .with_prompt(format!("{env}"))
                                //                 .interact_text()
                                //                 .unwrap_or_else(|_| std::process::exit(0))
                                //         }
                                //     })
                                //     .to_string();

                                if send_figterm(command.clone(), true).await.is_err() {
                                    let mut child =
                                        tokio::process::Command::new("bash").arg("-c").arg(command).spawn()?;
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
                    },
                    // choices => {
                    //     spinner.stop_with_message(format!("{spinner_text}{}", "<multiple options>".dark_grey()));
                    //     println!();

                    //     let mut actions: Vec<_> = choices
                    //         .iter()
                    //         .map(|choice| DialogActions::Execute {
                    //             command: choice.to_string(),
                    //             display: true,
                    //         })
                    //         .collect();

                    //     actions.extend_from_slice(&[
                    //         DialogActions::Regenerate,
                    //         DialogActions::Ask,
                    //         DialogActions::Cancel,
                    //     ]);

                    //     let selected = dialoguer::Select::with_theme(&crate::util::dialoguer_theme())
                    //         .default(0)
                    //         .items(&actions)
                    //         .interact_opt()?;

                    //     handle_action!(selected.and_then(|i| actions.get(i)));
                    // },
                }
            }
        }

        Ok(ExitCode::SUCCESS)
    }
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    #[ignore = "for manual eval testing"]
    async fn eval() {
        let prompts = [
            "list all files on desktop",
            "undo last commit",
            "create a new branch called test-feature",
            "check whether an app is codesigned correctly",
            "generate numbers 1 through 10",
            "determine whether an app is notarized correctly",
            "create a new git repository",
            "publish an npm package",
            "commit without running checks",
            "install vscode",
            "install vscode using brew",
            "write a loop that deletes all files in the current directory",
            "list all files larger than 2mb on my desktop",
            "quit finder",
            "restart finder",
            "find out what process is listening on port 3000",
            "kill the process listening on port 3000",
            "enumerate eks clusters",
            "list all versions of node that are installed on my computer",
            "one-line bash script to list all versions of node installed on my PATH",
            "upload this folder to s3",
            "stream logs for heroku app",
            "hide all icons on desktop macos",
            "list all usb connections",
            "delete all files on my computer",
            "list all usb connections using ioreg",
            "view a pr",
            "checkout 134 pr using gh",
            "checkout a pr using gh",
            "overwrite current branch with remote changes",
            "which is better: git or subversion",
            "add an alias to my dotiles",
            "Run the last command as root",
            "Serve the current directory on port 80",
            "Query google.com dns",
            "Portforward tcp port 25565",
            "create a new nextjs project",
            "create new react project called helloworld",
            "delete kubectl pod named hello-world",
            "send ping to unix socket",
            "Using the kubectl config at ~/.kube/kubconfig2 list all pods",
            "List kubernetes pods Sorted by Restart Count",
            "List all ubuntu aws instances",
            "Install onepassword with brew",
            "what is my ip",
            "clean terraform cache",
            "convert from main.pdf to md",
            "install nextjs 14 with turbopack",
            "compute md5 checksum of all files in directory output result in ${filename}.md5",
            "what is the capital of australia",
        ];

        for prompt in prompts {
            let res = generate_response(prompt, 1).await.unwrap();
            let first = res.completions.first().unwrap();
            std::println!("{prompt},{first}");
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    #[test]
    fn test_lints() {
        warning_message("sudo dd if=/dev/sda");
    }

    #[test]
    fn test_highlighter() {
        std::println!("{}", highlighter("echo $PATH $ABC $USER $HOME $DEF"));
    }
}
