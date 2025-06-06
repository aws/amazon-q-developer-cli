// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.
pub fn ser_supplemental_context(
    object: &mut ::aws_smithy_json::serialize::JsonObjectWriter,
    input: &crate::types::SupplementalContext,
) -> ::std::result::Result<(), ::aws_smithy_types::error::operation::SerializationError> {
    {
        object.key("filePath").string(input.file_path.as_str());
    }
    {
        object.key("content").string(input.content.as_str());
    }
    if let Some(var_1) = &input.r#type {
        object.key("type").string(var_1.as_str());
    }
    if let Some(var_2) = &input.metadata {
        #[allow(unused_mut)]
        let mut object_3 = object.key("metadata").start_object();
        crate::protocol_serde::shape_supplemental_context_metadata::ser_supplemental_context_metadata(
            &mut object_3,
            var_2,
        )?;
        object_3.finish();
    }
    Ok(())
}
