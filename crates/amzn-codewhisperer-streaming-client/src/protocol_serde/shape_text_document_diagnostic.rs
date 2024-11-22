// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.
pub fn ser_text_document_diagnostic(
    object: &mut ::aws_smithy_json::serialize::JsonObjectWriter,
    input: &crate::types::TextDocumentDiagnostic,
) -> Result<(), ::aws_smithy_types::error::operation::SerializationError> {
    {
        #[allow(unused_mut)]
        let mut object_1 = object.key("document").start_object();
        crate::protocol_serde::shape_text_document::ser_text_document(&mut object_1, &input.document)?;
        object_1.finish();
    }
    {
        #[allow(unused_mut)]
        let mut object_2 = object.key("range").start_object();
        crate::protocol_serde::shape_range::ser_range(&mut object_2, &input.range)?;
        object_2.finish();
    }
    {
        object.key("source").string(input.source.as_str());
    }
    {
        object.key("severity").string(input.severity.as_str());
    }
    {
        object.key("message").string(input.message.as_str());
    }
    Ok(())
}