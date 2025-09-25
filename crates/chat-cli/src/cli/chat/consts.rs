// These limits are the internal undocumented values from the service for each item

pub const MAX_CURRENT_WORKING_DIRECTORY_LEN: usize = 256;

/// Limit to send the number of messages as part of chat.
pub const MAX_CONVERSATION_STATE_HISTORY_LEN: usize = 250;

/// Actual service limit is 800_000
pub const MAX_TOOL_RESPONSE_SIZE: usize = 400_000;

/// Actual service limit is 600_000
pub const MAX_USER_MESSAGE_SIZE: usize = 400_000;

pub const DUMMY_TOOL_NAME: &str = "dummy";

/// Marker key used to identify invalid tool arguments (non-JSON objects)
pub const INVALID_TOOL_ARGS_MARKER: &str = "__error__invalid_args_json__";

pub const MAX_NUMBER_OF_IMAGES_PER_REQUEST: usize = 10;

/// In bytes - 10 MB
pub const MAX_IMAGE_SIZE: usize = 10 * 1024 * 1024;

pub const AGENT_FORMAT_HOOKS_DOC_URL: &str =
    "https://github.com/aws/amazon-q-developer-cli/blob/main/docs/agent-format.md#hooks-field";

pub const AGENT_FORMAT_TOOLS_DOC_URL: &str =
    "https://github.com/aws/amazon-q-developer-cli/blob/main/docs/agent-format.md#tools-field";

pub const AGENT_MIGRATION_DOC_URL: &str =
    "https://github.com/aws/amazon-q-developer-cli/blob/main/docs/legacy-profile-to-agent-migration.md";

// The environment variable name where we set additional metadata for the AWS CLI user agent.
pub const USER_AGENT_ENV_VAR: &str = "AWS_EXECUTION_ENV";
pub const USER_AGENT_APP_NAME: &str = "AmazonQ-For-CLI";
pub const USER_AGENT_VERSION_KEY: &str = "Version";
pub const USER_AGENT_VERSION_VALUE: &str = env!("CARGO_PKG_VERSION");
