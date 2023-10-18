// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.
pub fn ser_error_details(
    object: &mut ::aws_smithy_json::serialize::JsonObjectWriter,
    input: &crate::types::ErrorDetails,
) -> Result<(), ::aws_smithy_http::operation::error::SerializationError> {
    if let Some(var_1) = &input.command {
        object.key("command").string(var_1.as_str());
    }
    if let Some(var_2) = &input.epoch_timestamp {
        object.key("epochTimestamp").number(
            #[allow(clippy::useless_conversion)]
            ::aws_smithy_types::Number::NegInt((*var_2).into()),
        );
    }
    if let Some(var_3) = &input.r#type {
        object.key("type").string(var_3.as_str());
    }
    if let Some(var_4) = &input.message {
        object.key("message").string(var_4.as_str());
    }
    if let Some(var_5) = &input.stack_trace {
        object.key("stackTrace").string(var_5.as_str());
    }
    Ok(())
}