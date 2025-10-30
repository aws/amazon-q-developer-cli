use std::ffi::OsString;
use crate::os::Env;

/// Get environment variable as String
pub fn get_var(key: &str) -> Result<String, std::env::VarError> {
    std::env::var(key)
}

/// Get environment variable as OsString
pub fn get_var_os(key: &str) -> Option<OsString> {
    std::env::var_os(key)
}

/// Get environment variable with default value
pub fn get_var_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

// Semantic helpers that abstract away the actual variable names
pub fn get_log_level() -> Result<String, std::env::VarError> {
    get_var("Q_LOG_LEVEL")
}

#[cfg(unix)]
pub fn get_chat_shell() -> String {
    get_var_or("AMAZON_Q_CHAT_SHELL", "bash")
}

pub fn is_log_stdout_enabled() -> bool {
    get_var_os("Q_LOG_STDOUT").is_some()
}

pub fn is_telemetry_disabled() -> bool {
    get_var_os("Q_DISABLE_TELEMETRY").is_some()
}

pub fn get_mock_chat_response(env: Option<&Env>) -> Option<String> {
    match env {
        Some(e) => e.get("Q_MOCK_CHAT_RESPONSE").ok(),
        None => get_var("Q_MOCK_CHAT_RESPONSE").ok(),
    }
}

pub fn is_truecolor_disabled() -> bool {
    get_var_os("Q_DISABLE_TRUECOLOR").is_some_and(|s| !s.is_empty())
}

pub fn is_remote_fake() -> bool {
    get_var_os("Q_FAKE_IS_REMOTE").is_some()
}

pub fn in_codespaces() -> bool {
    get_var_os("CODESPACES").is_some() || get_var_os("Q_CODESPACES").is_some()
}

pub fn in_ci() -> bool {
    get_var_os("CI").is_some() || get_var_os("Q_CI").is_some()
}

pub fn get_cli_client_application() -> Option<String> {
    get_var("Q_CLI_CLIENT_APPLICATION").ok()
}

pub fn get_editor() -> String {
    get_var_or("EDITOR", "vi")
}

pub fn try_get_editor() -> Result<String, std::env::VarError> {
    get_var("EDITOR")
}

pub fn get_term() -> Option<String> {
    get_var("TERM").ok()
}

pub fn get_aws_region() -> Result<String, std::env::VarError> {
    get_var("AWS_REGION")
}

pub fn is_sigv4_enabled(env: Option<&Env>) -> bool {
    match env {
        Some(e) => e.get("AMAZON_Q_SIGV4").is_ok_and(|v| !v.is_empty()),
        None => get_var("AMAZON_Q_SIGV4").is_ok_and(|v| !v.is_empty()),
    }
}

pub fn get_all_env_vars() -> std::env::Vars {
    std::env::vars()
}

pub fn get_telemetry_client_id(env: Option<&Env>) -> Result<String, std::env::VarError> {
    match env {
        Some(e) => e.get("Q_TELEMETRY_CLIENT_ID"),
        None => get_var("Q_TELEMETRY_CLIENT_ID"),
    }
}
