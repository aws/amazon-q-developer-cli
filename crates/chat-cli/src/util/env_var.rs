use crate::os::Env;
use crate::util::consts::env_var::*;

/// Get log level from environment
pub fn get_log_level(env: &Env) -> Result<String, std::env::VarError> {
    env.get(Q_LOG_LEVEL)
}

/// Get chat shell with default fallback
#[cfg(unix)]
pub fn get_chat_shell() -> String {
    Env::new()
        .get(AMAZON_Q_CHAT_SHELL)
        .unwrap_or_else(|_| "bash".to_string())
}

/// Check if stdout logging is enabled
pub fn is_log_stdout_enabled() -> bool {
    Env::new().get_os(Q_LOG_STDOUT).is_some()
}

/// Check if telemetry is disabled
pub fn is_telemetry_disabled() -> bool {
    Env::new().get_os(Q_DISABLE_TELEMETRY).is_some()
}

/// Get mock chat response for testing
pub fn get_mock_chat_response(env: &Env) -> Option<String> {
    env.get(Q_MOCK_CHAT_RESPONSE).ok()
}

/// Check if truecolor is disabled
pub fn is_truecolor_disabled() -> bool {
    Env::new().get_os(Q_DISABLE_TRUECOLOR).is_some_and(|s| !s.is_empty())
}

/// Check if remote environment is faked
pub fn is_remote_fake() -> bool {
    Env::new().get_os(Q_FAKE_IS_REMOTE).is_some()
}

/// Check if running in Codespaces
pub fn in_codespaces() -> bool {
    let env = Env::new();
    env.get_os(CODESPACES).is_some() || env.get_os(Q_CODESPACES).is_some()
}

/// Check if running in CI
pub fn in_ci() -> bool {
    let env = Env::new();
    env.get_os(CI).is_some() || env.get_os(Q_CI).is_some()
}

/// Get CLI client application
pub fn get_cli_client_application() -> Option<String> {
    Env::new().get(Q_CLI_CLIENT_APPLICATION).ok()
}

/// Get editor with default fallback
pub fn get_editor() -> String {
    Env::new().get(EDITOR).unwrap_or_else(|_| "vi".to_string())
}

/// Try to get editor without fallback
pub fn try_get_editor() -> Result<String, std::env::VarError> {
    Env::new().get(EDITOR)
}

/// Get terminal type
pub fn get_term() -> Option<String> {
    Env::new().get(TERM).ok()
}

/// Get AWS region
pub fn get_aws_region() -> Result<String, std::env::VarError> {
    Env::new().get(AWS_REGION)
}

/// Check if SigV4 authentication is enabled
pub fn is_sigv4_enabled(env: &Env) -> bool {
    env.get(AMAZON_Q_SIGV4).is_ok_and(|v| !v.is_empty())
}

/// Get all environment variables
pub fn get_all_env_vars() -> std::env::Vars {
    std::env::vars()
}

/// Get telemetry client ID
pub fn get_telemetry_client_id(env: &Env) -> Result<String, std::env::VarError> {
    env.get(Q_TELEMETRY_CLIENT_ID)
}
