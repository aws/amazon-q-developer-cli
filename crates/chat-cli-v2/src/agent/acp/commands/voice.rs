use agent::tui_commands::{
    CommandResult,
    VoiceArgs,
};

use super::CommandContext;
use crate::database::settings::Setting;

pub async fn execute(args: &VoiceArgs, ctx: &CommandContext<'_>) -> CommandResult {
    if args.continuous {
        return CommandResult::error("Continuous voice mode is not yet supported in v2");
    }

    let server_url = ctx.os.database.settings.get_string(Setting::VoiceServerUrl);
    let model_size = ctx.os.database.settings.get_string(Setting::VoiceModelSize);
    let silence_timeout = ctx
        .os
        .database
        .settings
        .get_int(Setting::VoiceSilenceTimeout)
        .map(|v| v as u64);
    let max_session_time = ctx
        .os
        .database
        .settings
        .get_int(Setting::VoiceMaxSessionTime)
        .map(|v| v as u64);

    // Get context hint from recent conversation messages for better transcription accuracy
    let context_hint = match ctx.agent.create_snapshot().await {
        Ok(mut snapshot) => {
            let messages = snapshot.conversation_state.messages();
            if messages.is_empty() {
                None
            } else {
                let hint: String = messages
                    .iter()
                    .rev()
                    .take(5)
                    .filter_map(|m| m.content.iter().find_map(|b| b.text()))
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>()
                    .join("\n");
                let hint = if hint.len() > 500 {
                    hint.chars()
                        .rev()
                        .take(500)
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .collect::<String>()
                } else {
                    hint
                };
                if hint.is_empty() { None } else { Some(hint) }
            }
        },
        Err(_) => None,
    };

    // VoiceHandler contains cpal::Stream which is not Send — run in an OS thread
    let rt = tokio::runtime::Handle::current();
    let (tx, rx) = tokio::sync::oneshot::channel();

    std::thread::spawn(move || {
        let result = rt.block_on(async move {
            use voice::VoiceHandler;
            use voice::transcription_provider::TranscriptionBackend;

            let backend = TranscriptionBackend::LocalWhisper;

            let handler_result = VoiceHandler::new(
                backend,
                context_hint.clone(),
                model_size.clone(),
                silence_timeout,
                max_session_time,
            )
            .await;

            let mut voice_handler = match handler_result {
                Ok(handler) => match handler.check_setup().await {
                    Ok(()) => handler,
                    Err(e) if server_url.is_some() => {
                        let remote_backend = TranscriptionBackend::RemoteServer {
                            url: server_url.clone().unwrap(),
                        };
                        match VoiceHandler::new(remote_backend, context_hint, None, silence_timeout, max_session_time)
                            .await
                        {
                            Ok(h) => h,
                            Err(e2) => return CommandResult::error(format!("Remote voice server also failed: {e2}")),
                        }
                    },
                    Err(e) => {
                        return CommandResult::error(format!(
                            "No microphone found: {e}\n\
                                 On macOS: press Fn twice for dictation\n\
                                 On Windows: press Win+H for dictation\n\
                                 For cloud desktops: run `kiro-cli voice-cloud-setup <your-desktop-host>` from a machine with a microphone"
                        ));
                    },
                },
                Err(_) if server_url.is_some() => {
                    let remote_backend = TranscriptionBackend::RemoteServer {
                        url: server_url.clone().unwrap(),
                    };
                    match VoiceHandler::new(remote_backend, context_hint, None, silence_timeout, max_session_time).await
                    {
                        Ok(handler) => handler,
                        Err(e) => return CommandResult::error(format!("Voice init failed: {e}")),
                    }
                },
                Err(e) => {
                    return CommandResult::error(format!(
                        "No microphone found: {e}\n\
                         On macOS: press Fn twice for dictation\n\
                         On Windows: press Win+H for dictation\n\
                         For cloud desktops: run `kiro-cli voice-cloud-setup <your-desktop-host>` from a machine with a microphone"
                    ));
                },
            };

            let result = if voice_handler.is_remote() {
                voice_handler.listen_remote().await
            } else {
                voice_handler.listen_for_speech(false).await
            };

            match result {
                Ok(Some(text)) => {
                    let text = text.trim_start_matches(['!', '/']).trim().to_string();
                    if text.is_empty() {
                        CommandResult::success("No speech detected")
                    } else {
                        CommandResult::success_with_data(
                            "Voice transcription complete",
                            serde_json::json!({ "transcription": text }),
                        )
                    }
                },
                Ok(None) => CommandResult::success("No speech detected — try again with /voice"),
                Err(e) => CommandResult::error(format!("Voice recording failed: {e}")),
            }
        });
        let _ = tx.send(result);
    });

    rx.await
        .unwrap_or_else(|_| CommandResult::error("Voice recording thread panicked"))
}
