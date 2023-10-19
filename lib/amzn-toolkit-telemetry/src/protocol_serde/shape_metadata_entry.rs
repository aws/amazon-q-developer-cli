// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.
pub fn ser_metadata_entry(
    object: &mut ::aws_smithy_json::serialize::JsonObjectWriter,
    input: &crate::types::MetadataEntry,
) -> Result<(), ::aws_smithy_http::operation::error::SerializationError> {
    if let Some(var_1) = &input.key {
        object.key("key").string(var_1.as_str());
    }
    if let Some(var_2) = &input.value {
        object.key("value").string(var_2.as_str());
    }
    Ok(())
}