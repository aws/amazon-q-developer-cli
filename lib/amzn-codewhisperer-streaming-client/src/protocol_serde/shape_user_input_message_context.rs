// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.
pub fn ser_user_input_message_context(
    object: &mut ::aws_smithy_json::serialize::JsonObjectWriter,
    input: &crate::types::UserInputMessageContext,
) -> Result<(), ::aws_smithy_types::error::operation::SerializationError> {
    if let Some(var_1) = &input.editor_state {
        #[allow(unused_mut)]
        let mut object_2 = object.key("editorState").start_object();
        crate::protocol_serde::shape_editor_state::ser_editor_state(&mut object_2, var_1)?;
        object_2.finish();
    }
    if let Some(var_3) = &input.diagnostic {
        #[allow(unused_mut)]
        let mut object_4 = object.key("diagnostic").start_object();
        crate::protocol_serde::shape_diagnostic::ser_diagnostic(&mut object_4, var_3)?;
        object_4.finish();
    }
    Ok(())
}