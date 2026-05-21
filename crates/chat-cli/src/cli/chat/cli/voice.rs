use clap::Args;
use crossterm::style::{
    self,
    Color,
};
use crossterm::{
    cursor,
    execute,
};
use voice::providers::local_whisper::LocalWhisperProvider;

use crate::cli::chat::util::ui::draw_box;
use crate::cli::chat::voice::{
    TranscriptionBackend,
    VoiceHandler,
};
use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::database::settings::Setting;
use crate::os::Os;
use crate::theme::theme;

#[derive(Debug, PartialEq, Args)]
pub struct VoiceArgs {
    /// Transcription model to use
    #[arg(long, value_enum, default_value = "local-whisper")]
    pub model: TranscriptionBackendArg,

    /// Keep listening between turns (exit with Ctrl+C)
    #[arg(long)]
    pub continuous: bool,
}

#[derive(Debug, Clone, PartialEq, clap::ValueEnum)]
pub enum TranscriptionBackendArg {
    #[value(name = "local-whisper")]
    LocalWhisper,
    /// Use a remote voice server (for cloud desktops without a microphone)
    #[value(name = "remote")]
    Remote,
}

impl TranscriptionBackendArg {
    fn into_backend(self, server_url: Option<String>) -> TranscriptionBackend {
        match self {
            TranscriptionBackendArg::LocalWhisper => TranscriptionBackend::LocalWhisper,
            TranscriptionBackendArg::Remote => TranscriptionBackend::RemoteServer {
                url: server_url.unwrap_or_else(|| "http://localhost:19876".to_string()),
            },
        }
    }
}

impl VoiceArgs {
    pub async fn execute(self, os: &mut Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        // Toggle continuous mode
        if self.continuous {
            if session.continuous_voice {
                session.continuous_voice = false;
                execute!(
                    session.stderr,
                    style::SetForegroundColor(Color::Yellow),
                    style::Print("Continuous voice mode disabled\n"),
                    style::SetForegroundColor(Color::Reset)
                )?;
                return Ok(ChatState::PromptUser {
                    skip_printing_tools: true,
                });
            }
            session.continuous_voice = true;
            execute!(
                session.stderr,
                style::SetForegroundColor(Color::Green),
                style::Print("Continuous voice mode enabled (Ctrl+C to stop)\n"),
                style::SetForegroundColor(Color::Reset)
            )?;
        }

        let auto_submit = os.database.settings.get_bool(Setting::VoiceAutoSubmit).unwrap_or(false);

        // Show first-use help box (only once)
        let seen_welcome = os.database.settings.get_string(Setting::VoiceSeenWelcome).is_some();
        if !seen_welcome {
            let model_size = os
                .database
                .settings
                .get_string(Setting::VoiceModelSize)
                .unwrap_or_else(|| "base".to_string());
            let submit_label = if auto_submit { "ON" } else { "OFF" };
            let content = format!(
                "ENTER to stop · Ctrl+C to cancel\n\
                 Ctrl+O for voice · hold Space for PTT\n\
                 Auto-submit: {submit_label} · Model: whisper {model_size}.en"
            );
            draw_box(&mut session.stderr, "Voice", &content, 60, theme().ui.secondary_text).ok();
            execute!(session.stderr, style::Print("\n"))?;
            let _ = os.database.settings.set(Setting::VoiceSeenWelcome, "true", None).await;
        }

        let context_hint = session.voice_context_hint();
        let model_size = os.database.settings.get_string(Setting::VoiceModelSize);
        let server_url = os.database.settings.get_string(Setting::VoiceServerUrl);
        let silence_timeout = os
            .database
            .settings
            .get_int(Setting::VoiceSilenceTimeout)
            .map(|v| v as u64);
        let max_session_time = os
            .database
            .settings
            .get_int(Setting::VoiceMaxSessionTime)
            .map(|v| v as u64);
        let backend = self.model.clone().into_backend(server_url.clone());

        // If model needs downloading, download it and return — don't auto-start recording.
        if matches!(backend, TranscriptionBackend::LocalWhisper) {
            let size = model_size.as_deref().unwrap_or("base");
            if !LocalWhisperProvider::model_ready(size) {
                LocalWhisperProvider::ensure_model(size)
                    .await
                    .map_err(|e| ChatError::Custom(format!("Model download failed: {e}").into()))?;
                execute!(
                    session.stderr,
                    style::SetForegroundColor(Color::Green),
                    style::Print("\nVoice model ready! "),
                    style::SetForegroundColor(Color::Reset),
                    style::Print("Use "),
                    style::SetForegroundColor(Color::Cyan),
                    style::Print("/voice"),
                    style::SetForegroundColor(Color::Reset),
                    style::Print(", "),
                    style::SetForegroundColor(Color::Cyan),
                    style::Print("Ctrl+O"),
                    style::SetForegroundColor(Color::Reset),
                    style::Print(", or hold "),
                    style::SetForegroundColor(Color::Cyan),
                    style::Print("Space"),
                    style::SetForegroundColor(Color::Reset),
                    style::Print(" to start recording.\n\n")
                )?;
                return Ok(ChatState::PromptUser {
                    skip_printing_tools: true,
                });
            }
        }

        // Try the requested backend first
        let handler_result = VoiceHandler::new(
            backend,
            context_hint.clone(),
            model_size.clone(),
            silence_timeout,
            max_session_time,
        )
        .await;

        // If local mic fails and a remote server URL is configured, auto-fallback
        let mut voice_handler = match handler_result {
            Ok(handler) => match handler.check_setup().await {
                Ok(()) => handler,
                Err(e) if !handler.is_remote() && server_url.is_some() => {
                    execute!(
                        session.stderr,
                        style::SetForegroundColor(Color::DarkYellow),
                        style::Print("No local microphone — using remote voice server\n"),
                        style::SetForegroundColor(Color::Reset)
                    )?;
                    let remote_backend = TranscriptionBackend::RemoteServer {
                        url: server_url.unwrap(),
                    };
                    VoiceHandler::new(
                        remote_backend,
                        context_hint,
                        model_size,
                        silence_timeout,
                        max_session_time,
                    )
                    .await
                    .map_err(|e2| ChatError::Custom(format!("Remote voice server also failed: {e2}").into()))?
                },
                Err(e) => {
                    session.continuous_voice = false;
                    execute!(
                        session.stderr,
                        style::SetForegroundColor(Color::Red),
                        style::Print("No microphone found\n"),
                        style::SetForegroundColor(Color::DarkYellow),
                        style::Print("  💡 On macOS: press Fn twice for dictation\n"),
                        style::Print("  💡 On Windows: press Win+H for dictation\n"),
                        style::SetForegroundColor(Color::DarkGrey),
                        style::Print(format!("  ({})\n", e)),
                        style::SetForegroundColor(Color::Reset)
                    )?;
                    return Ok(ChatState::PromptUser {
                        skip_printing_tools: true,
                    });
                },
            },
            Err(_) if self.model == TranscriptionBackendArg::LocalWhisper && server_url.is_some() => {
                execute!(
                    session.stderr,
                    style::SetForegroundColor(Color::DarkYellow),
                    style::Print("No local microphone — using remote voice server\n"),
                    style::SetForegroundColor(Color::Reset)
                )?;
                let remote_backend = TranscriptionBackend::RemoteServer {
                    url: server_url.unwrap(),
                };
                VoiceHandler::new(
                    remote_backend,
                    context_hint,
                    model_size,
                    silence_timeout,
                    max_session_time,
                )
                .await
                .map_err(|e2| ChatError::Custom(format!("Remote voice server also failed: {e2}").into()))?
            },
            Err(e) => {
                session.continuous_voice = false;
                execute!(
                    session.stderr,
                    style::SetForegroundColor(Color::Red),
                    style::Print(format!("Failed to initialize voice: {}\n", e)),
                    style::SetForegroundColor(Color::Reset)
                )?;
                return Ok(ChatState::PromptUser {
                    skip_printing_tools: true,
                });
            },
        };

        // Hide cursor during recording so the volume bar renders cleanly
        let _ = execute!(session.stderr, cursor::Hide);

        let ptt_mode = session.ptt_voice_mode;
        session.ptt_voice_mode = false;

        let result = if voice_handler.is_remote() {
            voice_handler.listen_remote().await
        } else {
            voice_handler.listen_for_speech(ptt_mode).await
        };

        // Always restore cursor after recording
        let _ = execute!(session.stderr, cursor::Show);

        match result {
            Ok(Some(voice_input)) => {
                if auto_submit {
                    // Strip leading command chars only when auto-submitting directly
                    let voice_input = voice_input.trim_start_matches(['!', '/']).trim().to_string();
                    if voice_input.is_empty() {
                        return Ok(ChatState::PromptUser {
                            skip_printing_tools: true,
                        });
                    }
                    // Write directly to stderr (not the conduit) so it appears synchronously
                    // before readline draws the next prompt.
                    {
                        use std::io::Write as _;
                        eprintln!("\x1B[36m▶ {}\x1B[0m", &voice_input);
                        std::io::stderr().flush().ok();
                    }
                    Ok(ChatState::HandleInput { input: voice_input })
                } else {
                    // Non-auto-submit: text goes into the input box — no echo needed,
                    // the pre-filled readline line IS the visual confirmation.
                    let trimmed = voice_input.trim().to_string();
                    if trimmed.is_empty() {
                        return Ok(ChatState::PromptUser {
                            skip_printing_tools: true,
                        });
                    }
                    // If the user re-triggered PTT while a prior recording was showing,
                    // append the new text to what was already there.
                    let combined = match session.ptt_voice_prior_text.take() {
                        Some(prior) if !prior.is_empty() => format!("{} {}", prior, trimmed),
                        _ => trimmed,
                    };
                    session.pending_voice_text = Some(combined);
                    Ok(ChatState::PromptUser {
                        skip_printing_tools: true,
                    })
                }
            },
            Ok(None) => {
                session.continuous_voice = false;
                // Preserve any prior text that was in the input box before recording
                // (matches TUI: no speech → input stays unchanged).
                if let Some(prior) = session.ptt_voice_prior_text.take()
                    && !prior.is_empty()
                {
                    session.pending_voice_text = Some(prior);
                }
                Ok(ChatState::PromptUser {
                    skip_printing_tools: true,
                })
            },
            Err(e) => {
                session.continuous_voice = false;
                // Preserve prior text on error (don't lose user's input)
                if let Some(prior) = session.ptt_voice_prior_text.take()
                    && !prior.is_empty()
                {
                    session.pending_voice_text = Some(prior);
                }
                execute!(
                    session.stderr,
                    style::SetForegroundColor(Color::Red),
                    style::Print(format!("Voice input failed: {}\n", e)),
                    style::SetForegroundColor(Color::Reset)
                )?;
                Ok(ChatState::PromptUser {
                    skip_printing_tools: true,
                })
            },
        }
    }
}
