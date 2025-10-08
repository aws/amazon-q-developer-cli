 # AWS CodeWhisperer Client Crates Analysis

 ## Overview

 Analysis of the AWS client crates used in the Q CLI project, their endpoints, operations, and architecture.

 ## Client Crates

 ### 1. amzn-codewhisperer-client
 **Purpose**: Non-streaming CodeWhisperer API client for standard operations

 **Key Operations** (`/src/operation/`):
 - `generate_completions` - Code completions
 - `create_profile`, `get_profile`, `list_available_profiles` - Profile management
 - `start_transformation`, `get_transformation` - Code transformations
 - `send_telemetry_event`, `push_telemetry_event` - Telemetry
 - `create_workspace`, `delete_workspace` - Workspace management
 - `start_code_analysis`, `get_code_analysis` - Code analysis/security scanning

 **Configuration**: Uses configurable endpoints via `endpoint_url()` method in config builder (`/src/config.rs:328-346`)

 ### 2. amzn-codewhisperer-streaming-client
 **Purpose**: Streaming version for real-time responses

 **Key Operations**:
 - `send_message` - Chat/conversation streaming
 - `generate_assistant_response` - AI assistant responses
 - `generate_task_assist_plan` - Task planning assistance
 - `export_result_archive` - Export functionality

 **Architecture**: Built on AWS Smithy with event streaming support (`/src/event_stream_serde.rs`, `/src/event_receiver.rs`)

 ### 3. amzn-consolas-client
 **Purpose**: Consolas service client for customizations and profile management

 **Key Operations**:
 - `create_customization`, `update_customization`, `delete_customization` - Custom model management
 - `generate_recommendations` - AI recommendations
 - `create_profile`, `update_profile`, `delete_profile` - Profile operations
 - `associate_customization_permission`, `disassociate_customization_permission` - Permission management
 - `vend_key_grant` - Key/access management

 ### 4. amzn-qdeveloper-streaming-client
 **Purpose**: Q Developer streaming client with SigV4 authentication support

 Similar operations to codewhisperer-streaming-client but with different authentication

 ### 5. amzn-toolkit-telemetry-client
 **Purpose**: Telemetry data collection and reporting

 **Key Operations**:
 - `post_metrics` - Send usage metrics
 - `post_feedback` - User feedback collection
 - `post_error_report` - Error reporting

 ### 6. aws-toolkit-telemetry-definitions
 **Purpose**: Telemetry schema definitions and code generation

 **Key Files**:
 - `/def.json` - Telemetry event definitions
 - `/build.rs` - Code generation from definitions

 ## Endpoints and Configuration

 ### Primary Endpoints
 From `/crates/chat-cli/src/api_client/endpoints.rs`:

 1. **Default Q Service**: `https://q.us-east-1.amazonaws.com` (line 19)
 2. **EU Region**: `https://q.eu-central-1.amazonaws.com/` (line 23)
 3. **OIDC Authentication**: `https://oidc.{region}.amazonaws.com` (`/auth/builder_id.rs:83`)
 4. **Telemetry**:
    - Production: `https://client-telemetry.us-east-1.amazonaws.com` (`/telemetry/mod.rs:135`)
    - Beta: `https://7zftft3lj2.execute-api.us-east-1.amazonaws.com/Beta` (`/telemetry/mod.rs:130`)
 5. **Semantic Search Models**: `https://desktop-release.q.us-east-1.amazonaws.com/models`

 ### Authentication URLs
 From `/crates/chat-cli/src/auth/consts.rs`:
 - **Public Builder ID**: `https://view.awsapps.com/start` (line 18)
 - **Internal Amazon**: `https://amzn.awsapps.com/start` (line 21)

 ## Architecture and Flow

 ### Client Initialization
 From `/api_client/mod.rs:100-200`:

 1. **Endpoint Resolution**: Uses `Endpoint::configured_value()` to determine correct endpoint based on user profile region or custom settings

 2. **Authentication Setup**:
    - **Bearer Token Mode** (default): Uses `BearerResolver` for CodeWhisperer clients
    - **SigV4 Mode** (when `AMAZON_Q_SIGV4` env var set): Uses AWS credentials chain for Q Developer client

 3. **Client Configuration**: Each client configured with:
    - HTTP client with custom interceptors
    - User agent override
    - Opt-out preferences
    - Retry policies and timeout configurations
    - Endpoint URL from resolved endpoint

 4. **Service Selection**: Based on environment variables:
    - Uses `CodewhispererStreamingClient` for bearer token auth
    - Uses `QDeveloperStreamingClient` for SigV4 auth
    - Falls back to non-streaming `CodewhispererClient` for certain operations

 ### Request Flow
 1. Client determines appropriate endpoint based on user profile/region
 2. Authenticates using either Bearer tokens (Builder ID) or SigV4 (AWS credentials)
 3. Routes requests to appropriate service client
 4. Handles streaming responses for chat/conversation APIs
 5. Sends telemetry data to separate telemetry endpoints

 ## Key Implementation Details

 - All clients are generated from AWS Smithy specifications
 - Endpoint configuration is centralized in `endpoints.rs`
 - Authentication supports both public Builder ID and internal Amazon SSO
 - Streaming clients use event-based architecture for real-time responses
 - Telemetry is handled separately with its own client and endpoints
 - Region-based endpoint selection for global deployment support