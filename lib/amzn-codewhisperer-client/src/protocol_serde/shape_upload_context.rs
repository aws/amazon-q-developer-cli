// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.
pub fn ser_upload_context(
    object_8: &mut ::aws_smithy_json::serialize::JsonObjectWriter,
    input: &crate::types::UploadContext,
) -> Result<(), ::aws_smithy_types::error::operation::SerializationError> {
    match input {
        crate::types::UploadContext::WeaverBirdPlanningUploadContext(inner) => {
            #[allow(unused_mut)]
            let mut object_1 = object_8.key("weaverBirdPlanningUploadContext").start_object();
            crate::protocol_serde::shape_weaver_bird_planning_upload_context::ser_weaver_bird_planning_upload_context(
                &mut object_1,
                inner,
            )?;
            object_1.finish();
        },
        crate::types::UploadContext::Unknown => {
            return Err(::aws_smithy_types::error::operation::SerializationError::unknown_variant("UploadContext"));
        },
    }
    Ok(())
}