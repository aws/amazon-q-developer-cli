// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.
#[allow(clippy::unnecessary_wraps)]
pub fn de_generate_assistant_response_http_response(
    response: &mut ::aws_smithy_runtime_api::http::Response,
) -> std::result::Result<
    crate::operation::generate_assistant_response::GenerateAssistantResponseOutput,
    crate::operation::generate_assistant_response::GenerateAssistantResponseError,
> {
    let mut _response_body = ::aws_smithy_types::body::SdkBody::taken();
    std::mem::swap(&mut _response_body, response.body_mut());
    let _response_body = &mut _response_body;

    let _response_status = response.status().as_u16();
    let _response_headers = response.headers();
    Ok({
        #[allow(unused_mut)]
        let mut output =
            crate::operation::generate_assistant_response::builders::GenerateAssistantResponseOutputBuilder::default();
        output = output.set_generate_assistant_response_response(Some(
            crate::protocol_serde::shape_generate_assistant_response_output::de_generate_assistant_response_response_payload(_response_body)?,
        ));
        output._set_request_id(::aws_types::request_id::RequestId::request_id(_response_headers).map(str::to_string));
        output
            .build()
            .map_err(crate::operation::generate_assistant_response::GenerateAssistantResponseError::unhandled)?
    })
}

#[allow(clippy::unnecessary_wraps)]
pub fn de_generate_assistant_response_http_error(
    _response_status: u16,
    _response_headers: &::aws_smithy_runtime_api::http::Headers,
    _response_body: &[u8],
) -> std::result::Result<
    crate::operation::generate_assistant_response::GenerateAssistantResponseOutput,
    crate::operation::generate_assistant_response::GenerateAssistantResponseError,
> {
    #[allow(unused_mut)]
    let mut generic_builder =
        crate::protocol_serde::parse_http_error_metadata(_response_status, _response_headers, _response_body)
            .map_err(crate::operation::generate_assistant_response::GenerateAssistantResponseError::unhandled)?;
    generic_builder = ::aws_types::request_id::apply_request_id(generic_builder, _response_headers);
    let generic = generic_builder.build();
    let error_code = match generic.code() {
        Some(code) => code,
        None => {
            return Err(
                crate::operation::generate_assistant_response::GenerateAssistantResponseError::unhandled(generic),
            );
        },
    };

    let _error_message = generic.message().map(|msg| msg.to_owned());
    Err(match error_code {
        "ThrottlingException" => {
            crate::operation::generate_assistant_response::GenerateAssistantResponseError::ThrottlingError({
                #[allow(unused_mut)]
                let mut tmp = {
                    #[allow(unused_mut)]
                    let mut output = crate::types::error::builders::ThrottlingErrorBuilder::default();
                    output = crate::protocol_serde::shape_throttling_exception::de_throttling_exception_json_err(
                        _response_body,
                        output,
                    )
                    .map_err(
                        crate::operation::generate_assistant_response::GenerateAssistantResponseError::unhandled,
                    )?;
                    let output = output.meta(generic);
                    crate::serde_util::throttling_exception_correct_errors(output)
                        .build()
                        .map_err(
                            crate::operation::generate_assistant_response::GenerateAssistantResponseError::unhandled,
                        )?
                };
                tmp
            })
        },
        "ValidationException" => {
            crate::operation::generate_assistant_response::GenerateAssistantResponseError::ValidationError({
                #[allow(unused_mut)]
                let mut tmp = {
                    #[allow(unused_mut)]
                    let mut output = crate::types::error::builders::ValidationErrorBuilder::default();
                    output = crate::protocol_serde::shape_validation_exception::de_validation_exception_json_err(
                        _response_body,
                        output,
                    )
                    .map_err(
                        crate::operation::generate_assistant_response::GenerateAssistantResponseError::unhandled,
                    )?;
                    let output = output.meta(generic);
                    crate::serde_util::validation_exception_correct_errors(output)
                        .build()
                        .map_err(
                            crate::operation::generate_assistant_response::GenerateAssistantResponseError::unhandled,
                        )?
                };
                tmp
            })
        },
        "InternalServerError" => {
            crate::operation::generate_assistant_response::GenerateAssistantResponseError::InternalServerError({
                #[allow(unused_mut)]
                let mut tmp =
                    {
                        #[allow(unused_mut)]
                        let mut output = crate::types::error::builders::InternalServerErrorBuilder::default();
                        output = crate::protocol_serde::shape_internal_server_error::de_internal_server_error_json_err(
                            _response_body,
                            output,
                        )
                        .map_err(
                            crate::operation::generate_assistant_response::GenerateAssistantResponseError::unhandled,
                        )?;
                        let output = output.meta(generic);
                        crate::serde_util::internal_server_error_correct_errors(output)
                    .build()
                    .map_err(crate::operation::generate_assistant_response::GenerateAssistantResponseError::unhandled)?
                    };
                tmp
            })
        },
        "AccessDeniedException" => {
            crate::operation::generate_assistant_response::GenerateAssistantResponseError::AccessDeniedError({
                #[allow(unused_mut)]
                let mut tmp = {
                    #[allow(unused_mut)]
                    let mut output = crate::types::error::builders::AccessDeniedErrorBuilder::default();
                    output = crate::protocol_serde::shape_access_denied_exception::de_access_denied_exception_json_err(
                        _response_body,
                        output,
                    )
                    .map_err(
                        crate::operation::generate_assistant_response::GenerateAssistantResponseError::unhandled,
                    )?;
                    let output = output.meta(generic);
                    crate::serde_util::access_denied_exception_correct_errors(output)
                        .build()
                        .map_err(
                            crate::operation::generate_assistant_response::GenerateAssistantResponseError::unhandled,
                        )?
                };
                tmp
            })
        },
        _ => crate::operation::generate_assistant_response::GenerateAssistantResponseError::generic(generic),
    })
}

pub fn ser_generate_assistant_response_input(
    input: &crate::operation::generate_assistant_response::GenerateAssistantResponseInput,
) -> Result<::aws_smithy_types::body::SdkBody, ::aws_smithy_types::error::operation::SerializationError> {
    let mut out = String::new();
    let mut object = ::aws_smithy_json::serialize::JsonObjectWriter::new(&mut out);
    crate::protocol_serde::shape_generate_assistant_response_input::ser_generate_assistant_response_input_input(
        &mut object,
        input,
    )?;
    object.finish();
    Ok(::aws_smithy_types::body::SdkBody::from(out))
}