---
name: metrics-and-telemetry
description: Kiro CLI telemetry, metrics, and Kibana dashboard documentation. Use when investigating errors, understanding telemetry events, or analyzing CLI metrics.
---

# Metrics and Telemetry

## Dashboards

Telemetry data is available on Kibana. _You must be on the VPN to view._ Select the "global" tenant when prompted.

* [Chat Dashboard](https://telemetry-externalprod.ide-toolkits.dev-tools.aws.dev/_plugin/kibana/app/dashboards?security_tenant=global#/view/ba643300-3bdf-11f0-81a5-5b380cd28ab3?_g=(filters%3A!()%2CrefreshInterval%3A(pause%3A!t%2Cvalue%3A0)%2Ctime%3A(from%3Anow-1M%2Cto%3Anow)))
* [Autocomplete Dashboard (old)](https://telemetry-externalprod.ide-toolkits.dev-tools.aws.dev/_plugin/kibana/app/dashboards#/view/7d632ba0-8452-11ee-a5e4-47e65b6d55d5?_g=h@c823129&_a=h@7b5e93b)

Oncall should only be concerned with the Chat dashboard.

See the [Client Telemetry runbook](https://w.amazon.com/bin/view/CodeWhisperer/Operations/Runbooks/Plugin/) for a larger overview of using Kibana.

### Quick Tips for Kibana

* "result" is `Failed` for exceptions. Generally, it is `Cancelled` for ctrl-c interrupts.
* "reason" field contains general reason codes. You can look up how to do wildcard searches (e.g. *Exception).
* "reasonDesc" contains loose descriptive data, and may contain request ids to reference.

## CloudWatch Alarms

[CloudWatch Alarms](https://isengard.amazon.com/federate?account=678005972646&role=ReadOnly&destination=https%3A%2F%2Fus-east-1.console.aws.amazon.com%2Fcloudwatch%2Fdeeplink.js%3Fregion%3Dus-east-1%23alarmsV2%3Aalarm%2FConsolasRTS-prod-IAD-ChatAlarms-ChatAPIs-Availability%2BAlarm-GenerateAssistantComponentExecution-CLI-Critical%3F~(search~%27cli))

## Telemetry Definitions

Legacy Toolkit telemetry events are defined in the shared
[telemetry_definitions.json](../../crates/kiro-telemetry-legacy/telemetry_definitions.json) catalog.
OTel metrics and logs are defined in `crates/kiro-telemetry-schema/schema/`.

### Main Telemetry Events

| Event | Description |
|-------|-------------|
| `codewhispererterminal_addChatMessage` | Emitted on every new request |
| `amazonq_messageResponseError` | Emitted whenever the global error handler receives a new error ([mod.rs#L798](https://github.com/aws/amazon-q-developer-cli/blob/main/crates/chat-cli/src/cli/chat/mod.rs#L798)) |
| `codewhispererterminal_toolUseSuggested` | Emitted once for every single tool use emitted by the model (multiple tool uses = multiple events) |
| `codewhispererterminal_recordUserTurnCompletion` | Emitted at the end of every user turn |

### Important Call-outs

* When tracking errors, `addChatMessage` is a subset of the errors emitted by `messageResponseError`
* `messageResponseError` contains _all_ application errors, not just errors related to API requests (e.g., ctrl+c handling)

## Performance Guardrails

Telemetry emission runs on request, turn, tool-use, and process-health paths, so new metrics should be cheap when disabled and bounded when enabled.

* Prefer schema-aware constructors in `kiro_telemetry::metric` and `kiro_telemetry::log` over hand-built attribute strings. The constructors centralize known-alias normalization for dynamic dimensions such as model id, tool name, feature name, command name, MCP server, status code, and KAS/V3 attribution.
* Add high-frequency measurements as accumulated turn/session summaries where possible. Avoid per-token, per-render, or per-stream-chunk emission unless the product signal genuinely requires that cardinality and cost.
* Normalize known aliases client-side; pass unknown values through raw. Distinct-value caps (`max_distinct` in the schema, `_other_` overflow) are enforced downstream by KUTS ingestion, not the client — do not pre-bucket free-form dimensions like `version_full`, `mode`, or `mcp_server_name` before emission. Closed enums remain only for genuinely fixed sets (outcomes, os_type, engine).
* Direct meta-meter accounting is enabled only when the direct meta-meter task is running. Product metrics still emit normally, but the client avoids accumulating unpublished local accounting records.
* Export-size validation uses a conservative cheap estimate first and only serializes records that may exceed the limit. Logs with large free-form fields should still be redacted and summarized before they reach the telemetry client.

## Understanding Errors

### Result Field Values

The CLI uses the **result** field on telemetry events for specifying whether or not an event refers to an error:

| Value | Meaning |
|-------|---------|
| `Succeeded` | Operation completed successfully |
| `Failed` | An error occurred (see error codes below) |
| `Canceled` | User interrupted with ctrl+c |

### Error Classification Fields

For telemetry events that refer to errors, the CLI defines two fields:

* **reason** - Specific error code (e.g., `Interrupted`, `QuotaBreachError`). These are modeled directly in the codebase - grep for the reason to find the definition.
* **reasonDesc** - Loose descriptive data. May contain request IDs for reference.

## Error Codes Reference

### Application Error Codes

| Error Code | Description |
|------------|-------------|
| `Interrupted` | User pressed ctrl+c. Generally safe to ignore. |
| `ContextWindowOverflow` | User sent a request that caused the context window to overflow |
| `ModelOverloadedError` | The requested model has had too much traffic recently |
| `MonthlyLimitReached` | User has reached their monthly limit |
| `CompactHistoryFailure` | Conversation compaction failed. Indicates a bug - compaction should never fail. |
| `QuotaBreachError` | Request rate limit was breached (too many requests in short period) |
| `NonInteractiveToolApproval` | Emitted for non-interactive sessions where tool approval is required |
| `RecvErrorUnexpectedToolUseEos` | Unexpected end of stream while receiving a tool use. Not revealed as error in service metrics - bedrock sometimes returns invalid JSON for complicated tool uses. |
| `RecvErrorStreamTimeout` | Error waiting for next event in stream after long wait time |
| `RecvErrorApiClient` | Unknown errors while consuming a response stream |
| `RecvErrorJson` | JSON deserialization error during stream processing (should no longer be emitted) |
| `RecvErrorToolValidationError` | Invalid JSON from LLM for tool uses - could cause conversation corruption |

### API Client Error Codes

These errors come from the generated Q API client:

| Error Code | Description |
|------------|-------------|
| `dispatch failure` | Request failed during dispatch. No HTTP response received. Request MAY have been sent. Usually due to user's local environment (e.g., VPN blocking requests). |
| `service error` | Error response received from the service |
| `request has timed out` | Request failed due to timeout. Request MAY have been sent and received. |
| `InternalServerException` | Internal server error |
| `AccessDeniedException` | Access denied |
| `ValidationException` | Validation error |
| `ThrottlingException` | Request was throttled |
| `BedrockError` | Error from Bedrock |

## Error Classification Table

| Error | User Error | System Error | Notes |
|-------|:----------:|:------------:|-------|
| Interrupted | ✓ | | |
| ContextWindowOverflow | ✓ | | |
| ModelOverloadedError | | ✓ | Needs Investigation |
| MonthlyLimitReached | ✓ | | Quota |
| CompactHistoryFailure | | ✓ | |
| QuotaBreachError | ✓ | | Rate limiting |
| NonInteractiveToolApproval | ✓ | | |
| RecvErrorUnexpectedToolUseEos | | ✓ | |
| RecvErrorToolValidationError | | | |
| RecvErrorStreamTimeout | | ✓ | |
| RecvErrorApiClient | | ✓ | |
| RecvErrorJson | | ✓ | |
| dispatch error | ✓ | | |
| service error | | ✓ | |
| request has timed out | | ✓ | |
| InternalServerException | | ✓ | |
| AccessDeniedException | ✓ | | |
| ValidationException | | ✓ | Needs Investigation |
| ThrottlingException | ✓ | | |
| BedrockError | | ✓ | |
