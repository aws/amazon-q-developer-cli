// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.
pub fn ser_telemetry_event(
    object_3: &mut ::aws_smithy_json::serialize::JsonObjectWriter,
    input: &crate::types::TelemetryEvent,
) -> Result<(), ::aws_smithy_types::error::operation::SerializationError> {
    match input {
        crate::types::TelemetryEvent::UserTriggerDecisionEvent(inner) => {
            #[allow(unused_mut)]
            let mut object_1 = object_3.key("userTriggerDecisionEvent").start_object();
            crate::protocol_serde::shape_user_trigger_decision_event::ser_user_trigger_decision_event(
                &mut object_1,
                inner,
            )?;
            object_1.finish();
        },
        crate::types::TelemetryEvent::CodeCoverageEvent(inner) => {
            #[allow(unused_mut)]
            let mut object_2 = object_3.key("codeCoverageEvent").start_object();
            crate::protocol_serde::shape_code_coverage_event::ser_code_coverage_event(&mut object_2, inner)?;
            object_2.finish();
        },
        crate::types::TelemetryEvent::UserModificationEvent(inner) => {
            #[allow(unused_mut)]
            let mut object_3 = object_3.key("userModificationEvent").start_object();
            crate::protocol_serde::shape_user_modification_event::ser_user_modification_event(&mut object_3, inner)?;
            object_3.finish();
        },
        crate::types::TelemetryEvent::CodeScanEvent(inner) => {
            #[allow(unused_mut)]
            let mut object_4 = object_3.key("codeScanEvent").start_object();
            crate::protocol_serde::shape_code_scan_event::ser_code_scan_event(&mut object_4, inner)?;
            object_4.finish();
        },
        crate::types::TelemetryEvent::CodeScanRemediationsEvent(inner) => {
            #[allow(unused_mut)]
            let mut object_5 = object_3.key("codeScanRemediationsEvent").start_object();
            crate::protocol_serde::shape_code_scan_remediations_event::ser_code_scan_remediations_event(
                &mut object_5,
                inner,
            )?;
            object_5.finish();
        },
        crate::types::TelemetryEvent::MetricData(inner) => {
            #[allow(unused_mut)]
            let mut object_6 = object_3.key("metricData").start_object();
            crate::protocol_serde::shape_metric_data::ser_metric_data(&mut object_6, inner)?;
            object_6.finish();
        },
        crate::types::TelemetryEvent::ChatAddMessageEvent(inner) => {
            #[allow(unused_mut)]
            let mut object_7 = object_3.key("chatAddMessageEvent").start_object();
            crate::protocol_serde::shape_chat_add_message_event::ser_chat_add_message_event(&mut object_7, inner)?;
            object_7.finish();
        },
        crate::types::TelemetryEvent::ChatInteractWithMessageEvent(inner) => {
            #[allow(unused_mut)]
            let mut object_8 = object_3.key("chatInteractWithMessageEvent").start_object();
            crate::protocol_serde::shape_chat_interact_with_message_event::ser_chat_interact_with_message_event(
                &mut object_8,
                inner,
            )?;
            object_8.finish();
        },
        crate::types::TelemetryEvent::ChatUserModificationEvent(inner) => {
            #[allow(unused_mut)]
            let mut object_9 = object_3.key("chatUserModificationEvent").start_object();
            crate::protocol_serde::shape_chat_user_modification_event::ser_chat_user_modification_event(
                &mut object_9,
                inner,
            )?;
            object_9.finish();
        },
        crate::types::TelemetryEvent::TerminalUserInteractionEvent(inner) => {
            #[allow(unused_mut)]
            let mut object_10 = object_3.key("terminalUserInteractionEvent").start_object();
            crate::protocol_serde::shape_terminal_user_interaction_event::ser_terminal_user_interaction_event(
                &mut object_10,
                inner,
            )?;
            object_10.finish();
        },
        crate::types::TelemetryEvent::FeatureDevEvent(inner) => {
            #[allow(unused_mut)]
            let mut object_11 = object_3.key("featureDevEvent").start_object();
            crate::protocol_serde::shape_feature_dev_event::ser_feature_dev_event(&mut object_11, inner)?;
            object_11.finish();
        },
        crate::types::TelemetryEvent::Unknown => {
            return Err(::aws_smithy_types::error::operation::SerializationError::unknown_variant("TelemetryEvent"));
        },
    }
    Ok(())
}
