use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use base64::prelude::*;
use bytes::BytesMut;
use fig_proto::fig::server_originated_message::Submessage as ServerOriginatedSubMessage;
use fig_proto::fig::{
    EditBufferChangedNotification,
    HistoryUpdatedNotification,
    KeybindingPressedNotification,
    LocationChangedNotification,
    Notification,
    NotificationType,
    Process,
    ProcessChangedNotification,
    ServerOriginatedMessage,
    ShellPromptReturnedNotification,
};
use fig_proto::local::{
    EditBufferHook,
    InterceptedKeyHook,
    PostExecHook,
    PreExecHook,
    PromptHook,
};
use fig_proto::prost::Message;
use fig_proto::remote::clientbound;
use fig_remote_ipc::figterm::{
    FigtermState,
    SessionMetrics,
};
use time::OffsetDateTime;
use tracing::{
    debug,
    error,
};
use uuid::Uuid;

use crate::event::{
    EmitEventName,
    Event,
    WindowEvent,
};
use crate::platform::PlatformBoundEvent;
use crate::webview::notification::WebviewNotificationsState;
use crate::{
    AUTOCOMPLETE_ID,
    EventLoopProxy,
};

#[derive(Debug, Clone)]
pub struct RemoteHook {
    pub notifications_state: Arc<WebviewNotificationsState>,
    pub proxy: EventLoopProxy,
}

#[async_trait::async_trait]
impl fig_remote_ipc::RemoteHookHandler for RemoteHook {
    type Error = anyhow::Error;

    async fn edit_buffer(
        &mut self,
        hook: &EditBufferHook,
        session_id: Uuid,
        figterm_state: &Arc<FigtermState>,
    ) -> Result<Option<clientbound::response::Response>> {
        let _old_metrics = figterm_state.with_update(session_id, |session| {
            session.edit_buffer.text.clone_from(&hook.text);
            session.edit_buffer.cursor.clone_from(&hook.cursor);
            session
                .terminal_cursor_coordinates
                .clone_from(&hook.terminal_cursor_coordinates);
            session.context.clone_from(&hook.context);

            let received_at = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
            let current_session_expired = session
                .current_session_metrics
                .as_ref()
                .is_some_and(|metrics| received_at > metrics.end_time + Duration::from_secs(5));

            if current_session_expired {
                let previous = session.current_session_metrics.clone();
                session.current_session_metrics = Some(SessionMetrics::new(received_at));
                previous
            } else {
                if let Some(ref mut metrics) = session.current_session_metrics {
                    metrics.end_time = received_at;
                }
                None
            }
        });

        let utf16_cursor_position = hook
            .text
            .get(..hook.cursor as usize)
            .map(|s| s.encode_utf16().count() as i32);

        for sub in self.notifications_state.subscriptions.iter() {
            let message_id = match sub.get(&NotificationType::NotifyOnEditbuffferChange) {
                Some(id) => *id,
                None => continue,
            };

            let hook = hook.clone();
            let message = ServerOriginatedMessage {
                id: Some(message_id),
                submessage: Some(ServerOriginatedSubMessage::Notification(Notification {
                    r#type: Some(fig_proto::fig::notification::Type::EditBufferNotification(
                        EditBufferChangedNotification {
                            context: hook.context,
                            buffer: Some(hook.text),
                            cursor: utf16_cursor_position,
                            session_id: Some(session_id.into()),
                        },
                    )),
                })),
            };

            let mut encoded = BytesMut::new();
            message.encode(&mut encoded).unwrap();

            debug!(%message_id, "Sending edit buffer change notification to webview");

            self.proxy
                .send_event(Event::WindowEvent {
                    window_id: sub.key().clone(),
                    window_event: WindowEvent::Emit {
                        event_name: EmitEventName::Notification,
                        payload: BASE64_STANDARD.encode(encoded).into(),
                    },
                })
                .unwrap();
        }

        let empty_edit_buffer = hook.text.trim().is_empty();

        if !empty_edit_buffer {
            self.proxy
                .send_event(Event::PlatformBoundEvent(PlatformBoundEvent::EditBufferChanged))?;
        }

        self.proxy.send_event(Event::WindowEvent {
            window_id: AUTOCOMPLETE_ID,
            // If editbuffer is empty, hide the autocomplete window to avoid flickering
            window_event: if empty_edit_buffer {
                WindowEvent::Hide
            } else {
                WindowEvent::Show
            },
        })?;

        Ok(None)
    }

    async fn prompt(
        &mut self,
        hook: &PromptHook,
        session_id: Uuid,
        figterm_state: &Arc<FigtermState>,
    ) -> Result<Option<clientbound::response::Response>> {
        let mut cwd_changed = false;
        let mut new_cwd = None;
        figterm_state.with(&session_id, |session| {
            if let (Some(old_context), Some(new_context)) = (&session.context, &hook.context) {
                cwd_changed = old_context.current_working_directory != new_context.current_working_directory;
                new_cwd.clone_from(&new_context.current_working_directory);
            }

            session.context.clone_from(&hook.context);
        });

        if cwd_changed {
            if let Err(err) = self
                .notifications_state
                .broadcast_notification_all(
                    &NotificationType::NotifyOnLocationChange,
                    Notification {
                        r#type: Some(fig_proto::fig::notification::Type::LocationChangedNotification(
                            LocationChangedNotification {
                                session_id: Some(session_id.to_string()),
                                host_name: hook.context.as_ref().and_then(|ctx| ctx.hostname.clone()),
                                user_name: None,
                                directory: new_cwd,
                            },
                        )),
                    },
                    &self.proxy,
                )
                .await
            {
                error!(%err, "Failed to broadcast LocationChangedNotification");
            }
        }

        if let Err(err) = self
            .notifications_state
            .broadcast_notification_all(
                &NotificationType::NotifyOnPrompt,
                Notification {
                    r#type: Some(fig_proto::fig::notification::Type::ShellPromptReturnedNotification(
                        ShellPromptReturnedNotification {
                            session_id: Some(session_id.to_string()),
                            shell: hook.context.as_ref().map(|ctx| Process {
                                pid: ctx.pid,
                                executable: ctx.process_name.clone(),
                                directory: ctx.current_working_directory.clone(),
                                env: vec![],
                            }),
                        },
                    )),
                },
                &self.proxy,
            )
            .await
        {
            error!(%err, "Failed to broadcast ShellPromptReturnedNotification");
        }

        Ok(None)
    }

    async fn pre_exec(
        &mut self,
        hook: &PreExecHook,
        session_id: Uuid,
        figterm_state: &Arc<FigtermState>,
    ) -> Result<Option<clientbound::response::Response>> {
        figterm_state.with_update(session_id, |session| {
            session.context.clone_from(&hook.context);
        });

        self.proxy.send_event(Event::WindowEvent {
            window_id: AUTOCOMPLETE_ID.clone(),
            window_event: WindowEvent::Hide,
        })?;

        self.notifications_state
            .broadcast_notification_all(
                &NotificationType::NotifyOnProcessChanged,
                Notification {
                    r#type: Some(fig_proto::fig::notification::Type::ProcessChangeNotification(
                        ProcessChangedNotification {
                        session_id: Some(session_id.to_string()),
                        new_process: // TODO: determine active application based on tty
                        hook.context.as_ref().map(|ctx| Process {
                            pid: ctx.pid,
                            executable: ctx.process_name.clone(),
                            directory: ctx.current_working_directory.clone(),
                            env: vec![],
                        }),
                    },
                    )),
                },
                &self.proxy,
            )
            .await?;

        Ok(None)
    }

    async fn post_exec(
        &mut self,
        hook: &PostExecHook,
        session_id: Uuid,
        figterm_state: &Arc<FigtermState>,
    ) -> Result<Option<clientbound::response::Response>> {
        figterm_state.with_update(session_id, |session| {
            session.context.clone_from(&hook.context);
        });

        self.notifications_state
            .broadcast_notification_all(
                &NotificationType::NotifyOnHistoryUpdated,
                Notification {
                    r#type: Some(fig_proto::fig::notification::Type::HistoryUpdatedNotification(
                        HistoryUpdatedNotification {
                            command: hook.command.clone(),
                            process_name: hook.context.as_ref().and_then(|ctx| ctx.process_name.clone()),
                            current_working_directory: hook
                                .context
                                .as_ref()
                                .and_then(|ctx| ctx.current_working_directory.clone()),
                            session_id: Some(session_id.to_string()),
                            hostname: hook.context.as_ref().and_then(|ctx| ctx.hostname.clone()),
                            exit_code: hook.exit_code,
                        },
                    )),
                },
                &self.proxy,
            )
            .await?;

        Ok(None)
    }

    async fn intercepted_key(
        &mut self,
        InterceptedKeyHook { action, context, .. }: InterceptedKeyHook,
        _session_id: Uuid,
    ) -> Result<Option<clientbound::response::Response>> {
        debug!(%action, "Intercepted Key Action");

        self.notifications_state
            .broadcast_notification_all(
                &NotificationType::NotifyOnKeybindingPressed,
                Notification {
                    r#type: Some(fig_proto::fig::notification::Type::KeybindingPressedNotification(
                        KeybindingPressedNotification {
                            keypress: None,
                            action: Some(action),
                            context,
                        },
                    )),
                },
                &self.proxy,
            )
            .await?;

        Ok(None)
    }
}
