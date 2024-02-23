// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.
pub fn ser_chat_user_modification_event(
    object: &mut ::aws_smithy_json::serialize::JsonObjectWriter,
    input: &crate::types::ChatUserModificationEvent,
) -> Result<(), ::aws_smithy_types::error::operation::SerializationError> {
    {
        object.key("conversationId").string(input.conversation_id.as_str());
    }
    {
        object.key("messageId").string(input.message_id.as_str());
    }
    if let Some(var_1) = &input.programming_language {
        #[allow(unused_mut)]
        let mut object_2 = object.key("programmingLanguage").start_object();
        crate::protocol_serde::shape_programming_language::ser_programming_language(&mut object_2, var_1)?;
        object_2.finish();
    }
    {
        object.key("modificationPercentage").number(
            #[allow(clippy::useless_conversion)]
            ::aws_smithy_types::Number::Float((input.modification_percentage).into()),
        );
    }
    Ok(())
}