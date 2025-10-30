use crate::cli::chat::cli::model::context_window_tokens;
use crate::cli::chat::token_counter::{
    CharCount,
    TokenCount,
};
use crate::cli::chat::{
    ChatError,
    ChatSession,
};
use crate::os::Os;

/// Get detailed usage data for context window analysis
pub(super) async fn get_detailed_usage_data(
    session: &mut ChatSession,
    os: &Os,
) -> Result<super::DetailedUsageData, ChatError> {
    let context_window_size = context_window_tokens(session.conversation.model_info.as_ref());

    let state = session
        .conversation
        .backend_conversation_state(os, true, &mut std::io::stderr())
        .await?;

    let data = state.calculate_conversation_size();
    let tool_specs_json: String = state
        .tools
        .values()
        .filter_map(|s| serde_json::to_string(s).ok())
        .collect::<Vec<String>>()
        .join("");
    let tools_char_count: CharCount = tool_specs_json.len().into();
    let total_tokens: TokenCount =
        (data.context_messages + data.user_messages + data.assistant_messages + tools_char_count).into();

    Ok(super::DetailedUsageData {
        total_tokens,
        context_tokens: data.context_messages.into(),
        assistant_tokens: data.assistant_messages.into(),
        user_tokens: data.user_messages.into(),
        tools_tokens: tools_char_count.into(),
        context_window_size,
        dropped_context_files: state.dropped_context_files,
    })
}

/// Get total usage percentage (external API)
pub async fn get_total_usage_percentage(session: &mut ChatSession, os: &Os) -> Result<f32, ChatError> {
    let data = get_detailed_usage_data(session, os).await?;
    Ok((data.total_tokens.value() as f32 / data.context_window_size as f32) * 100.0)
}
