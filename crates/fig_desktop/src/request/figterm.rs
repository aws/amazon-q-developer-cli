use fig_proto::fig::InsertTextRequest;
use fig_remote_ipc::figterm::{
    FigtermCommand,
    FigtermState,
};
use uuid::Uuid;

use super::{
    RequestResult,
    RequestResultImpl,
};

pub async fn insert_text(request: InsertTextRequest, state: &FigtermState) -> RequestResult {
    let figterm_command = match request.r#type {
        Some(some) => match some {
            fig_proto::fig::insert_text_request::Type::Text(text) => FigtermCommand::InsertText {
                insertion: Some(text),
                deletion: None,
                immediate: None,
                offset: None,
                insertion_buffer: None,
                insert_during_command: None,
            },
            fig_proto::fig::insert_text_request::Type::Update(update) => FigtermCommand::InsertText {
                insertion: update.insertion,
                deletion: update.deletion,
                immediate: update.immediate,
                offset: update.offset,
                insertion_buffer: update.insertion_buffer,
                insert_during_command: None,
            },
        },
        None => return RequestResult::error("InsertTextRequest expects a request type"),
    };

    let uuid_str = request.terminal_session_id.ok_or("terminal_session_id is required")?;
    let uuid = Uuid::parse_str(&uuid_str).map_err(|err| format!("terminal_session_id is not a valid UUID: {err}"))?;

    match state.with(&uuid, |session| session.sender.clone()) {
        Some(sender) => {
            sender
                .send(figterm_command)
                .map_err(|err| format!("Failed sending command to figterm session: {err}"))?;
            RequestResult::success()
        },
        None => RequestResult::error("No figterm sessions"),
    }
}
