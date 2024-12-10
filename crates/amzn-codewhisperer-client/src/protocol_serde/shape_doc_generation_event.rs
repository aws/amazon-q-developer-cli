// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.
pub fn ser_doc_generation_event(
    object: &mut ::aws_smithy_json::serialize::JsonObjectWriter,
    input: &crate::types::DocGenerationEvent,
) -> Result<(), ::aws_smithy_types::error::operation::SerializationError> {
    {
        object.key("conversationId").string(input.conversation_id.as_str());
    }
    if input.number_of_add_chars != 0 {
        object.key("numberOfAddChars").number(
            #[allow(clippy::useless_conversion)]
            ::aws_smithy_types::Number::NegInt((input.number_of_add_chars).into()),
        );
    }
    if input.number_of_add_lines != 0 {
        object.key("numberOfAddLines").number(
            #[allow(clippy::useless_conversion)]
            ::aws_smithy_types::Number::NegInt((input.number_of_add_lines).into()),
        );
    }
    if input.number_of_add_files != 0 {
        object.key("numberOfAddFiles").number(
            #[allow(clippy::useless_conversion)]
            ::aws_smithy_types::Number::NegInt((input.number_of_add_files).into()),
        );
    }
    if let Some(var_1) = &input.user_decision {
        object.key("userDecision").string(var_1.as_str());
    }
    if let Some(var_2) = &input.interaction_type {
        object.key("interactionType").string(var_2.as_str());
    }
    if let Some(var_3) = &input.user_identity {
        object.key("userIdentity").string(var_3.as_str());
    }
    if input.number_of_navigation != 0 {
        object.key("numberOfNavigation").number(
            #[allow(clippy::useless_conversion)]
            ::aws_smithy_types::Number::NegInt((input.number_of_navigation).into()),
        );
    }
    if let Some(var_4) = &input.folder_level {
        object.key("folderLevel").string(var_4.as_str());
    }
    Ok(())
}