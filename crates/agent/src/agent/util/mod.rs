pub mod consts;
pub mod directories;
pub mod error;
pub mod glob;
pub mod path;
pub mod providers;
pub mod request_channel;
pub mod test;

use std::collections::HashMap;
use std::env::VarError;
use std::os::unix::fs::MetadataExt as _;
use std::path::Path;

use bstr::ByteSlice as _;
use consts::env_var::CLI_IS_INTEG_TEST;
use error::{
    ErrorContext as _,
    UtilError,
};
use regex::Regex;
use tokio::io::{
    AsyncReadExt as _,
    BufReader,
};

pub fn expand_env_vars(env_vars: &mut HashMap<String, String>) {
    let env_provider = |input: &str| Ok(std::env::var(input).ok());
    expand_env_vars_impl(env_vars, env_provider);
}

fn expand_env_vars_impl<E>(env_vars: &mut HashMap<String, String>, env_provider: E)
where
    E: Fn(&str) -> Result<Option<String>, VarError>,
{
    // Create a regex to match ${env:VAR_NAME} pattern
    let re = Regex::new(r"\$\{env:([^}]+)\}").unwrap();
    for (_, value) in env_vars.iter_mut() {
        *value = re
            .replace_all(value, |caps: &regex::Captures<'_>| {
                let var_name = &caps[1];
                env_provider(var_name)
                    .unwrap_or_else(|_| Some(format!("${{{}}}", var_name)))
                    .unwrap_or_else(|| format!("${{{}}}", var_name))
            })
            .to_string();
    }
}

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
pub fn truncate_safe_in_place(s: &mut String, max_bytes: usize, suffix: &str) {
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

    let truncated_amount = if md.size() > max_file_length {
        // Edge case check to ensure the suffix is less than max file length.
        if suffix.len() as u64 > max_file_length {
            return Ok((String::new(), md.size()));
        }
        md.size() - max_file_length + suffix.len() as u64
    } else {
        0
    };

    if truncated_amount == 0 {
        return Ok((content, 0));
    }

    content.replace_range((content.len().saturating_sub(suffix.len())).., suffix);
    Ok((content, truncated_amount))
}

pub fn is_integ_test() -> bool {
    std::env::var_os(CLI_IS_INTEG_TEST).is_some_and(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::util::test::TestDir;

    #[test]
    fn test_truncate_safe() {
        assert_eq!(truncate_safe("Hello World", 5), "Hello");
        assert_eq!(truncate_safe("Hello ", 5), "Hello");
        assert_eq!(truncate_safe("Hello World", 11), "Hello World");
        assert_eq!(truncate_safe("Hello World", 15), "Hello World");
    }

    #[test]
    fn test_truncate_safe_in_place() {
        let suffix = "suffix";
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

        expand_env_vars_impl(&mut env_vars, env_provider);

        assert_eq!(env_vars.get("KEY1").unwrap(), "Value is test_value");
        assert_eq!(env_vars.get("KEY2").unwrap(), "No substitution");
    }

    #[tokio::test]
    async fn test_read_file_with_max_limit() {
        // Test file with 30 bytes in length
        let test_file = "123456789\n".repeat(3);
        let test_provider = crate::util::test::TestProvider::new();
        let d = TestDir::new().with_file_sys(("test.txt", &test_file), &test_provider).await;

        // Test not truncated
        let (content, bytes_truncated) = read_file_with_max_limit(d.join("test.txt"), 100, "...").await.unwrap();
        assert_eq!(content, test_file);
        assert_eq!(bytes_truncated, 0);

        // Test truncated
        let (content, bytes_truncated) = read_file_with_max_limit(d.join("test.txt"), 10, "...").await.unwrap();
        assert_eq!(content, "1234567...");
        assert_eq!(bytes_truncated, 23);

        // Test suffix greater than max length
        let (content, bytes_truncated) = read_file_with_max_limit(d.join("test.txt"), 1, "...").await.unwrap();
        assert_eq!(content, "");
        assert_eq!(bytes_truncated, 30);
    }
}
