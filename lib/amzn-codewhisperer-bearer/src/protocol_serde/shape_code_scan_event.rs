// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.
pub fn ser_code_scan_event(
    object: &mut ::aws_smithy_json::serialize::JsonObjectWriter,
    input: &crate::types::CodeScanEvent,
) -> Result<(), ::aws_smithy_http::operation::error::SerializationError> {
    if let Some(var_1) = &input.programming_language {
        #[allow(unused_mut)]
        let mut object_2 = object.key("programmingLanguage").start_object();
        crate::protocol_serde::shape_programming_language::ser_programming_language(&mut object_2, var_1)?;
        object_2.finish();
    }
    if let Some(var_3) = &input.code_scan_job_id {
        object.key("codeScanJobId").string(var_3.as_str());
    }
    if let Some(var_4) = &input.timestamp {
        object
            .key("timestamp")
            .date_time(var_4, ::aws_smithy_types::date_time::Format::EpochSeconds)?;
    }
    Ok(())
}