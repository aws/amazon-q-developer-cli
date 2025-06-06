// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.
pub fn ser_workspace_state(
    object: &mut ::aws_smithy_json::serialize::JsonObjectWriter,
    input: &crate::types::WorkspaceState,
) -> ::std::result::Result<(), ::aws_smithy_types::error::operation::SerializationError> {
    {
        object.key("uploadId").string(input.upload_id.as_str());
    }
    {
        #[allow(unused_mut)]
        let mut object_1 = object.key("programmingLanguage").start_object();
        crate::protocol_serde::shape_programming_language::ser_programming_language(
            &mut object_1,
            &input.programming_language,
        )?;
        object_1.finish();
    }
    if let Some(var_2) = &input.context_truncation_scheme {
        object.key("contextTruncationScheme").string(var_2.as_str());
    }
    Ok(())
}
