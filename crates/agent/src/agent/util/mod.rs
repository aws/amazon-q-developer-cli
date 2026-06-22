pub mod cancel_guard;
pub mod consts;
pub mod directories;
pub mod error;
pub mod glob;
pub mod path;
pub mod providers;
pub mod request_channel;
pub mod shell;
pub mod steering;
pub mod test;

use std::collections::HashMap;
use std::env::VarError;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt as _;
#[cfg(windows)]
use std::os::windows::fs::MetadataExt as _;
use std::path::Path;
use std::sync::LazyLock;

/// Cross-platform helper to get file size from metadata
#[cfg(unix)]
fn get_file_size(md: &std::fs::Metadata) -> u64 {
    md.size()
}

#[cfg(windows)]
fn get_file_size(md: &std::fs::Metadata) -> u64 {
    md.file_size()
}

use bstr::ByteSlice as _;
use consts::env_var::{
    ACP_CLIENT_NAME_ENV_VAR,
    CLI_IS_INTEG_TEST,
};
use consts::{
    ACP_CLIENT_UA_TOKEN,
    USER_AGENT_APP_NAME,
    USER_AGENT_ENV_VAR,
    USER_AGENT_VERSION_KEY,
    USER_AGENT_VERSION_VALUE,
};
use error::{
    ErrorContext as _,
    UtilError,
};
use regex::Regex;
use tokio::io::{
    AsyncReadExt as _,
    BufReader,
};

pub const DEFAULT_TRUNCATE_SUFFIX: &str = "...truncated";

pub fn expand_env_vars(env_vars: &mut HashMap<String, String>) {
    let env_provider = |input: &str| Ok(std::env::var(input).ok());
    expand_env_vars_impl(env_vars, env_provider);
}

fn expand_env_vars_impl<E>(env_vars: &mut HashMap<String, String>, env_provider: E)
where
    E: Fn(&str) -> Result<Option<String>, VarError>,
{
    static ENV_VAR_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\$\{(?:env:)?([^}]+)\}").unwrap());
    for (_, value) in env_vars.iter_mut() {
        *value = ENV_VAR_REGEX
            .replace_all(value, |caps: &regex::Captures<'_>| {
                let var_name = &caps[1];
                env_provider(var_name)
                    .unwrap_or_else(|_| Some(format!("${{{var_name}}}")))
                    .unwrap_or_else(|| format!("${{{var_name}}}"))
            })
            .to_string();
    }
}

#[allow(clippy::string_slice)] // byte_count is from char_indices(), always a valid char boundary
pub fn truncate_safe(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }

    let mut byte_count = 0;
    let mut char_indices = s.char_indices();

    for (byte_idx, _) in &mut char_indices {
        if byte_count + (byte_idx - byte_count) > max_bytes {
            break;
        }
        byte_count = byte_idx;
    }

    &s[..byte_count]
}

/// Truncates `s` to a maximum length of `max_bytes`, appending `suffix` if `s` was truncated. The
/// result is always guaranteed to be at least less than `max_bytes`.
///
/// If both `s` and `suffix` are larger than `max_bytes`, then `s` is replaced with a truncated
/// `suffix`.
pub fn truncate_safe_in_place(s: &mut String, max_bytes: usize, suffix: Option<&str>) {
    let suffix = suffix.unwrap_or(DEFAULT_TRUNCATE_SUFFIX);
    // If `s` doesn't need to be truncated, do nothing.
    if s.len() <= max_bytes {
        return;
    }

    // Replace `s` with a truncated suffix if both are greater than `max_bytes`.
    if s.len() > max_bytes && suffix.len() > max_bytes {
        let truncated_suffix = truncate_safe(suffix, max_bytes);
        s.replace_range(.., truncated_suffix);
        return;
    }

    let end = truncate_safe(s, max_bytes - suffix.len()).len();
    s.replace_range(end..s.len(), suffix);
    s.truncate(max_bytes);
}

/// Reads a file to a maximum file length, returning the content and number of bytes truncated. If
/// the file has to be truncated, content is suffixed with `truncated_suffix`.
///
/// The returned content length is guaranteed to not be greater than `max_file_length`.
pub async fn read_file_with_max_limit(
    path: impl AsRef<Path>,
    max_file_length: u64,
    truncated_suffix: impl AsRef<str>,
) -> Result<(String, u64), UtilError> {
    let path = path.as_ref();
    let suffix = truncated_suffix.as_ref();
    let file = tokio::fs::File::open(path)
        .await
        .with_context(|| format!("Failed to open file at '{}'", path.to_string_lossy()))?;
    let md = file
        .metadata()
        .await
        .with_context(|| format!("Failed to query file metadata at '{}'", path.to_string_lossy()))?;

    // Read only the max supported length.
    let mut reader = BufReader::new(file).take(max_file_length);
    let mut content = Vec::new();
    reader
        .read_to_end(&mut content)
        .await
        .with_context(|| format!("Failed to read from file at '{}'", path.to_string_lossy()))?;
    let mut content = content.to_str_lossy().to_string();

    let truncated_amount = if get_file_size(&md) > max_file_length {
        // Edge case check to ensure the suffix is less than max file length.
        if suffix.len() as u64 > max_file_length {
            return Ok((String::new(), get_file_size(&md)));
        }
        get_file_size(&md) - max_file_length + suffix.len() as u64
    } else {
        0
    };

    if truncated_amount == 0 {
        return Ok((content, 0));
    }

    let safe_end = truncate_safe(&content, content.len().saturating_sub(suffix.len())).len();
    content.replace_range(safe_end.., suffix);
    Ok((content, truncated_amount))
}

pub fn is_integ_test() -> bool {
    std::env::var_os(CLI_IS_INTEG_TEST).is_some_and(|s| !s.is_empty())
}

/// Builds the value of the `AWS_EXECUTION_ENV` user-agent header shared by every
/// `aws`-spawning tool.
///
/// `existing` is any caller-set value, preserved verbatim as a prefix.
/// `acp_client`, when present and non-empty, is appended as an
/// `acp-client/<name>` token so AWS CLI calls can be attributed to the driving
/// ACP client (e.g. in CloudTrail). When `acp_client` is `None` or empty the
/// output is byte-identical to the previous behavior.
///
/// Kept as a pure function so the three env builders share one formatter and
/// tests can exercise every branch without mutating the process environment
/// (which is `unsafe` and unsound under the multi-threaded test harness on
/// Rust ≥1.83).
pub fn build_user_agent_value(existing: Option<&str>, acp_client: Option<&str>) -> String {
    let metadata = format!("{USER_AGENT_APP_NAME} {USER_AGENT_VERSION_KEY}/{USER_AGENT_VERSION_VALUE}");
    let mut value = match existing {
        Some(v) if !v.is_empty() => format!("{v} {metadata}"),
        _ => metadata,
    };
    if let Some(client) = acp_client
        && !client.is_empty()
    {
        value.push_str(&format!(" {ACP_CLIENT_UA_TOKEN}/{client}"));
    }
    value
}

/// Inserts the shared `AWS_EXECUTION_ENV` user-agent value into an environment
/// map for a spawned `aws`-invoking child process.
///
/// Reads the existing `AWS_EXECUTION_ENV` and the driving-client name
/// (`ACP_CLIENT_NAME_ENV_VAR`) *from the map itself*. Callers build the map via
/// `std::env::vars().collect()`, so reading from the map is equivalent to
/// reading the process environment while avoiding a second lookup. The computed
/// value is written back under `USER_AGENT_ENV_VAR`.
///
/// This is the single point of truth for the three env builders (`use_aws`,
/// `execute_cmd` unix/windows); each keeps only its own extra keys. Behavior is
/// byte-identical to the previous inline logic, including when
/// `ACP_CLIENT_NAME_ENV_VAR` is unset (the token is simply omitted).
pub fn insert_user_agent(env_vars: &mut std::collections::HashMap<String, String>) {
    let value = build_user_agent_value(
        env_vars.get(USER_AGENT_ENV_VAR).map(String::as_str),
        env_vars.get(ACP_CLIENT_NAME_ENV_VAR).map(String::as_str),
    );
    env_vars.insert(USER_AGENT_ENV_VAR.to_string(), value);
}

/// Sanitizes an ACP client name for safe use as a user-agent token value.
///
/// Keeps only `[A-Za-z0-9._-]`, caps the result at 64 characters, and returns
/// `None` if nothing usable remains. The ASCII allowlist drops spaces, `/`,
/// newlines, NUL, other control characters, and unicode. This prevents two
/// distinct issues with the attacker-controlled `Initialize` name:
/// - a NUL byte would panic `set_var` (a denial-of-service vector), and
/// - spaces or `/` would let a name forge extra user-agent tokens in `AWS_EXECUTION_ENV` (e.g. `"x
///   acp-client/y"`).
///
/// Sanitization is lossy and not injective: distinct names can map to the same
/// token (e.g. `"a/b"` and `"ab"` both yield `"ab"`). This is acceptable for
/// best-effort attribution, which does not require a reversible mapping.
///
/// Pure and side-effect free so it can be unit-tested without mutating the
/// process environment.
pub fn sanitize_acp_client_name(name: &str) -> Option<String> {
    let cleaned: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        // Cap length: `AWS_EXECUTION_ENV` imposes no hard limit, so bound this
        // attacker-controlled token to keep the user-agent value from growing
        // unboundedly on hostile input.
        .take(64)
        .collect();
    if cleaned.is_empty() { None } else { Some(cleaned) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_safe() {
        assert_eq!(truncate_safe("Hello World", 5), "Hello");
        assert_eq!(truncate_safe("Hello ", 5), "Hello");
        assert_eq!(truncate_safe("Hello World", 11), "Hello World");
        assert_eq!(truncate_safe("Hello World", 15), "Hello World");
    }

    #[test]
    fn test_truncate_safe_in_place() {
        let suffix = Some("suffix");
        let tests = &[
            ("Hello World", 7, "Hsuffix"),
            ("Hello World", usize::MAX, "Hello World"),
            // test for when suffix is too large
            ("hi", 5, "hi"),
            ("Hello World", 5, "suffi"),
            // α -> 2 byte length
            ("αααααα", 7, "suffix"),
            ("αααααα", 8, "αsuffix"),
            ("αααααα", 9, "αsuffix"),
        ];
        assert!("α".len() == 2);

        for (orig_input, max_bytes, expected) in tests {
            let mut input = (*orig_input).to_string();
            truncate_safe_in_place(&mut input, *max_bytes, suffix);
            assert_eq!(
                input.as_str(),
                *expected,
                "input: {} with max bytes: {} failed",
                orig_input,
                max_bytes
            );
        }
    }

    #[tokio::test]
    async fn test_process_env_vars() {
        // stub env vars
        let mut vars = HashMap::new();
        vars.insert("TEST_VAR".to_string(), "test_value".to_string());
        let env_provider = |var: &str| Ok(vars.get(var).cloned());

        // value under test
        let mut env_vars = HashMap::new();
        env_vars.insert("KEY1".to_string(), "Value is ${env:TEST_VAR}".to_string());
        env_vars.insert("KEY2".to_string(), "No substitution".to_string());
        env_vars.insert("KEY3".to_string(), "${TEST_VAR}".to_string());

        expand_env_vars_impl(&mut env_vars, env_provider);

        assert_eq!(env_vars.get("KEY1").unwrap(), "Value is test_value");
        assert_eq!(env_vars.get("KEY2").unwrap(), "No substitution");
        assert_eq!(env_vars.get("KEY3").unwrap(), "test_value");
    }

    #[tokio::test]
    async fn test_read_file_with_max_limit() {
        // Test file with 30 bytes in length
        let test_file = "123456789\n".repeat(3);
        let test_base = crate::util::test::TestBase::new()
            .await
            .with_file(("test.txt", &test_file))
            .await;

        // Test not truncated
        let (content, bytes_truncated) = read_file_with_max_limit(test_base.join("test.txt"), 100, "...")
            .await
            .unwrap();
        assert_eq!(content, test_file);
        assert_eq!(bytes_truncated, 0);

        // Test truncated
        let (content, bytes_truncated) = read_file_with_max_limit(test_base.join("test.txt"), 10, "...")
            .await
            .unwrap();
        assert_eq!(content, "1234567...");
        assert_eq!(bytes_truncated, 23);

        // Test suffix greater than max length
        let (content, bytes_truncated) = read_file_with_max_limit(test_base.join("test.txt"), 1, "...")
            .await
            .unwrap();
        assert_eq!(content, "");
        assert_eq!(bytes_truncated, 30);
    }

    #[tokio::test]
    async fn test_read_file_with_max_limit_multibyte() {
        // File with emoji near the end: "hello🎉x" = 5 + 4 + 1 = 10 bytes
        let test_file = "hello🎉x";
        let test_base = crate::util::test::TestBase::new()
            .await
            .with_file(("test.txt", test_file))
            .await;

        // max_limit=9 the correct answer is "hello..." which is 8 bytes
        // it should not try to read the 1st byte of 🎉 to get to 9 bytes. it should discard 🎉 entirely
        let (content, _) = read_file_with_max_limit(test_base.join("test.txt"), 9, "...")
            .await
            .unwrap();
        assert_eq!(content, "hello...");
    }

    #[test]
    fn test_is_integ_test_returns_bool() {
        // Just ensure it returns a bool without panicking
        let _ = is_integ_test();
    }

    #[test]
    fn test_expand_env_vars_no_match() {
        let mut vars = HashMap::new();
        vars.insert("KEY".to_string(), "no var here".to_string());
        expand_env_vars(&mut vars);
        assert_eq!(vars.get("KEY"), Some(&"no var here".to_string()));
    }

    #[tokio::test]
    async fn test_read_file_with_max_limit_no_truncation() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("small.txt");
        tokio::fs::write(&path, "hi").await.unwrap();
        let (content, truncated) = read_file_with_max_limit(&path, 1000, "...").await.unwrap();
        assert_eq!(content, "hi");
        assert_eq!(truncated, 0);
    }

    #[tokio::test]
    async fn test_read_file_with_max_limit_nonexistent() {
        let result = read_file_with_max_limit("/nonexistent/path", 100, "...").await;
        assert!(result.is_err());
    }

    #[test]
    fn sanitize_keeps_plain_name() {
        assert_eq!(sanitize_acp_client_name("meshclaw").as_deref(), Some("meshclaw"));
    }

    #[test]
    fn sanitize_drops_spaces() {
        // Spaces are stripped so a multiword name can't introduce extra UA tokens.
        assert_eq!(
            sanitize_acp_client_name("Visual Studio Code").as_deref(),
            Some("VisualStudioCode")
        );
    }

    #[test]
    fn sanitize_drops_spaces_and_slashes() {
        // A name crafted to forge tokens (spaces + `/`) must lose both.
        let out = sanitize_acp_client_name("x AmazonQ-For-CLI Version/9.9").expect("non-empty");
        assert!(!out.contains(' '), "result must not contain spaces: {out}");
        assert!(!out.contains('/'), "result must not contain slashes: {out}");
    }

    #[test]
    fn sanitize_strips_nul_without_panic() {
        // NUL would panic `set_var`; it must be removed, not preserved.
        assert_eq!(sanitize_acp_client_name("a\u{0}b").as_deref(), Some("ab"));
    }

    #[test]
    fn sanitize_returns_none_when_nothing_usable() {
        assert_eq!(sanitize_acp_client_name("///"), None);
        assert_eq!(sanitize_acp_client_name("   "), None);
        assert_eq!(sanitize_acp_client_name(""), None);
    }

    #[test]
    fn sanitize_caps_length_at_64() {
        let long = "a".repeat(200);
        assert_eq!(sanitize_acp_client_name(&long).map(|s| s.len()), Some(64));
    }

    #[test]
    fn sanitized_name_yields_exactly_one_acp_client_token() {
        // End-to-end: a messy multiword name, once sanitized, produces exactly one
        // well-formed `acp-client/<name>` token with no stray spaces inside it.
        let sanitized = sanitize_acp_client_name("Visual Studio Code").expect("non-empty");
        let ua = build_user_agent_value(None, Some(&sanitized));
        assert_eq!(
            ua.matches(" acp-client/").count(),
            1,
            "expected exactly one acp-client token, got: {ua}"
        );
        let token = ua
            .split(' ')
            .find(|t| t.starts_with("acp-client/"))
            .expect("token present");
        assert_eq!(token, "acp-client/VisualStudioCode");
    }
}
