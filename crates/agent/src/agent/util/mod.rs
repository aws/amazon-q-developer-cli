use std::collections::HashMap;
use std::env::VarError;

use consts::env_var::CLI_IS_INTEG_TEST;
use regex::Regex;

pub mod consts;
pub mod directories;
pub mod error;
pub mod glob;
pub mod path;
pub mod request_channel;

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
/// If `suffix` is larger than `max_bytes`, or `s` is within `max_bytes`, then this function does
/// nothing.
pub fn truncate_safe_in_place(s: &mut String, max_bytes: usize, suffix: &str) {
    // Do nothing if the suffix is too large to be truncated within max_bytes, or s is already small
    // enough to not be truncated.
    if suffix.len() > max_bytes || s.len() <= max_bytes {
        return;
    }

    let end = truncate_safe(s, max_bytes - suffix.len()).len();
    s.replace_range(end..s.len(), suffix);
    s.truncate(max_bytes);
}

pub fn is_integ_test() -> bool {
    std::env::var_os(CLI_IS_INTEG_TEST).is_some_and(|s| !s.is_empty())
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
        let suffix = "suffix";
        let tests = &[
            ("Hello World", 5, "Hello World"),
            ("Hello World", 7, "Hsuffix"),
            ("Hello World", usize::MAX, "Hello World"),
            // α -> 2 byte length
            ("αααααα", 7, "suffix"),
            ("αααααα", 8, "αsuffix"),
            ("αααααα", 9, "αsuffix"),
        ];
        assert!("α".len() == 2);

        for (input, max_bytes, expected) in tests {
            let mut input = (*input).to_string();
            truncate_safe_in_place(&mut input, *max_bytes, suffix);
            assert_eq!(
                input.as_str(),
                *expected,
                "input: {} with max bytes: {} failed",
                input,
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
}
