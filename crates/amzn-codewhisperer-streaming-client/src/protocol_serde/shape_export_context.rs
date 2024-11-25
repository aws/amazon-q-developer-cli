// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.
pub fn ser_export_context(
    object_4: &mut ::aws_smithy_json::serialize::JsonObjectWriter,
    input: &crate::types::ExportContext,
) -> Result<(), ::aws_smithy_types::error::operation::SerializationError> {
    match input {
        crate::types::ExportContext::TransformationExportContext(inner) => {
            #[allow(unused_mut)]
            let mut object_1 = object_4.key("transformationExportContext").start_object();
            crate::protocol_serde::shape_transformation_export_context::ser_transformation_export_context(
                &mut object_1,
                inner,
            )?;
            object_1.finish();
        },
        crate::types::ExportContext::Unknown => {
            return Err(::aws_smithy_types::error::operation::SerializationError::unknown_variant("ExportContext"));
        },
    }
    Ok(())
}
