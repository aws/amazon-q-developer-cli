//! # Chat Script Framework
//!
//! A JSON-based scripting system for testing AI chat interactions. Allows you to script
//! both MCP tool responses and LLM sampling requests with predictable, testable behavior.
//!
//! ## Overview
//!
//! Chat scripts define sequences of steps that can:
//! - **Return values** for MCP tool calls
//! - **Make LLM sampling requests** and validate responses
//! - **Call other scripts** for composition
//! - **Substitute arguments** using `${variable}` syntax
//!
//! ## Basic Example
//!
//! ```json
//! {
//!   "scripts": [
//!     {
//!       "name": "countryCapital",
//!       "arguments": ["country"],
//!       "steps": [
//!         {
//!           "Return": {
//!             "value": "The capital of ${country} is Athens"
//!           }
//!         }
//!       ]
//!     }
//!   ]
//! }
//! ```
//!
//! ## Script Steps
//!
//! ### Return Step
//! Returns a JSON value, ending script execution:
//! ```json
//! {
//!   "Return": {
//!     "value": "Athens"
//!   }
//! }
//! ```
//!
//! ### Call Step
//! Calls another script by name:
//! ```json
//! {
//!   "Call": {
//!     "name": "helper_script",
//!     "arguments": {
//!       "param1": "value1"
//!     }
//!   }
//! }
//! ```
//!
//! ### Sampling Step
//! Makes an LLM request and validates the response:
//! ```json
//! {
//!   "Sampling": {
//!     "params": {
//!       "messages": [{"role": "user", "content": "Hello"}]
//!     },
//!     "expected": {
//!       "Ok": {
//!         "regex": ".*greeting.*"
//!       }
//!     }
//!   }
//! }
//! ```
//!
//! ## Argument Substitution
//!
//! Scripts support variable substitution using `${variable}` syntax:
//! - `"${country}"` - Substitutes entire value with argument
//! - `"Hello ${name}!"` - String interpolation
//! - Works in all JSON values (strings, objects, arrays)
//!
//! ## Integration
//!
//! Implement `ScriptEnvironment` to handle LLM sampling requests:
//!
//! ```rust,ignore
//! struct MyEnvironment;
//!
//! impl ScriptEnvironment for MyEnvironment {
//!     fn send_sampling_request(
//!         &self,
//!         params: serde_json::Value,
//!         response_tx: tokio::sync::oneshot::Sender<Result<serde_json::Value, serde_json::Value>>,
//!     ) -> eyre::Result<()> {
//!         // Handle LLM request, send response via response_tx
//!         Ok(())
//!     }
//! }
//! ```

use std::{collections::BTreeMap, pin::Pin, future::Future};

use regex::Regex;
use serde::{Deserialize, Serialize};

pub trait ScriptEnvironment: Send + Sync {
    /// Send a sampling request to the client. When the result arrives, send it to `response_tx`.
    /// Send `Ok` if it is a success (with the params), `Err` if it is an error (with the serialize JSON RPC error).
    fn send_sampling_request(
        &self,
        params: serde_json::Value,
        response_tx: tokio::sync::oneshot::Sender<Result<serde_json::Value, serde_json::Value>>,
    ) -> eyre::Result<()>;
}

#[derive(Serialize, Deserialize)]
pub struct Scripts {
    scripts: Vec<Script>,
}

impl Scripts {
    /// Return the script (if any) with the given name.
    pub fn find_by_name(&self, name: &str) -> Option<&Script> {
        self.scripts.iter().find(|s| s.name == name)
    }

    /// If a script with the given name exists, call it with the given arguments.
    pub async fn execute_if_exists(
        &self,
        name: &str,
        arguments: BTreeMap<String, serde_json::Value>,
        env: &dyn ScriptEnvironment,
    ) -> eyre::Result<Option<serde_json::Value>> {
        if let Some(script) = self.find_by_name(name) {
            self.execute(script, arguments, env).await
        } else {
            Ok(Some(serde_json::Value::Null))
        }
    }

    /// Execute the given script with the given arguments.
    pub fn execute<'a>(
        &'a self,
        script: &'a Script,
        arguments: BTreeMap<String, serde_json::Value>,
        env: &'a dyn ScriptEnvironment,
    ) -> Pin<Box<dyn Future<Output = eyre::Result<Option<serde_json::Value>>> + Send + 'a>> {
        Box::pin(async move {
            // Check all required arguments (and only required arguments) were provided
            for arg in &script.arguments {
                if !arguments.contains_key(arg) {
                    eyre::bail!("Missing required argument: {arg}");
                }
            }
            for arg in arguments.keys() {
                if !script.arguments.contains(arg) {
                    eyre::bail!("Unknown argument: {arg}");
                }
            }

            // Create local copy of script with values substituted
            let script = Self::subst_arguments_in_script(script, &arguments)?;

            // Execute script steps
            for step in script.steps {
                match step {
                    Step::Call {
                        name: callee_name,
                        arguments: callee_arguments,
                    } => match self.find_by_name(&callee_name) {
                        Some(callee_script) => {
                            if let Some(result) = self.execute(callee_script, callee_arguments, env).await? {
                                return Ok(Some(result));
                            }
                        },
                        None => eyre::bail!("no script named {callee_name}"),
                    },

                    Step::Sampling { params, expected } => {
                        let (tx, rx) = tokio::sync::oneshot::channel();
                        env.send_sampling_request(params, tx)?;
                        let value = rx.await?;
                        Self::test_result(&expected, &value)?;
                    },

                    Step::Return { value } => {
                        return Ok(Some(value));
                    },
                }
            }
            Ok(None)
        })
    }

    fn subst_arguments_in_script(
        script: &Script,
        arguments: &BTreeMap<String, serde_json::Value>,
    ) -> eyre::Result<Script> {
        Ok(Script {
            name: script.name.clone(),
            arguments: script.arguments.clone(),
            steps: script
                .steps
                .iter()
                .map(|step| Self::subst_arguments_in_step(step, arguments))
                .collect::<eyre::Result<_>>()?,
        })
    }

    fn subst_arguments_in_step(step: &Step, arguments: &BTreeMap<String, serde_json::Value>) -> eyre::Result<Step> {
        match step {
            Step::Call { name, arguments } => Ok(Step::Call {
                name: name.clone(),
                arguments: arguments
                    .iter()
                    .map(|(k, v)| Ok((k.clone(), Self::subst_arguments_in_value(v, arguments)?)))
                    .collect::<eyre::Result<_>>()?,
            }),
            Step::Sampling { params, expected } => Ok(Step::Sampling {
                params: Self::subst_arguments_in_value(params, arguments)?,
                expected: match expected {
                    Ok(f) => Ok(Self::subst_argument_in_filter(f, arguments)?),
                    Err(f) => Err(Self::subst_argument_in_filter(f, arguments)?),
                },
            }),
            Step::Return { value } => Ok(Step::Return {
                value: Self::subst_arguments_in_value(value, arguments)?,
            }),
        }
    }

    fn test_result(
        filter: &Result<Filter, Filter>,
        value: &Result<serde_json::Value, serde_json::Value>,
    ) -> eyre::Result<()> {
        match (filter, value) {
            (Ok(f), Ok(v)) => Self::test_filter(f, v),
            (Err(f), Err(v)) => Self::test_filter(f, v),
            _ => eyre::bail!("Result does not match filter {filter:?}: {value:?}"),
        }
    }
    fn test_filter(filter: &Filter, value: &serde_json::Value) -> eyre::Result<()> {
        let re = Regex::new(&filter.regex)?;
        if !re.is_match(&value.to_string()) {
            eyre::bail!("Filter `{}` does not match value `{}`", filter.regex, value);
        }
        Ok(())
    }
    fn subst_argument_in_filter(
        filter: &Filter,
        arguments: &BTreeMap<String, serde_json::Value>,
    ) -> eyre::Result<Filter> {
        let Filter { regex } = filter;
        let regex = Self::subst_arguments_in_str(regex, arguments)?;
        Ok(Filter { regex })
    }

    /// For each `V=K` in the arguments, replace any instance of `${V}` with `K`.
    fn subst_arguments_in_value(
        value: &serde_json::Value,
        arguments: &BTreeMap<String, serde_json::Value>,
    ) -> eyre::Result<serde_json::Value> {
        match value {
            serde_json::Value::Null => Ok(value.clone()),
            serde_json::Value::Bool(_) => Ok(value.clone()),
            serde_json::Value::Number(_) => Ok(value.clone()),
            serde_json::Value::String(s) => {
                // Check if string is exactly ${{XXX}} pattern
                if s.starts_with("${") && s.ends_with("}") && s.len() > 5 {
                    let key = &s[2..s.len() - 1];
                    if let Some(value) = arguments.get(key) {
                        return Ok(value.clone());
                    } else {
                        eyre::bail!("Unknown argument: {key}");
                    }
                }

                // Otherwise do string substitution
                Ok(serde_json::Value::String(Self::subst_arguments_in_str(s, &arguments)?))
            },
            serde_json::Value::Array(values) => Ok(serde_json::Value::Array(
                values
                    .iter()
                    .map(|v| Self::subst_arguments_in_value(v, arguments))
                    .collect::<eyre::Result<_>>()?,
            )),
            serde_json::Value::Object(map) => Ok(serde_json::Value::Object(
                map.iter()
                    .map(|(k, v)| {
                        Ok((
                            Self::subst_arguments_in_str(k, arguments)?,
                            Self::subst_arguments_in_value(v, arguments)?,
                        ))
                    })
                    .collect::<eyre::Result<_>>()?,
            )),
        }
    }

    /// For each `V=K` in the arguments, replace any instance of `${V}` with `K`.
    fn subst_arguments_in_str(s: &str, arguments: &BTreeMap<String, serde_json::Value>) -> eyre::Result<String> {
        let r = Regex::new(r"\$\{([^}]+)\}").unwrap();
        let mut error = None;
        let s = r.replace_all(s, |caps: &regex::Captures<'_>| {
            let key = &caps[1];
            if let Some(value) = arguments.get(key) {
                match value {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                }
            } else {
                error = Some(key.to_string());
                String::new()
            }
        });

        if let Some(k) = error {
            eyre::bail!("error in `{s}`, key not found: `{k}`");
        }

        Ok(s.into_owned())
    }
}

/// *Scripts* are code that is interpreted by the MCP test server.
#[derive(Serialize, Deserialize)]
pub struct Script {
    /// Name of the script.
    ///
    /// If this is the same as the name of a tool, the MCP server will call it.
    ///
    /// If it is "main", the MCP server will call it on entry.
    name: String,

    /// Expected arguments
    arguments: Vec<String>,

    /// What steps should it follow when it executes
    steps: Vec<Step>,
}

#[derive(Serialize, Deserialize)]
enum Step {
    /// Call the script named 'name'
    Call {
        name: String,
        arguments: BTreeMap<String, serde_json::Value>,
    },

    /// Send a sampling request to the client
    Sampling {
        /// Request parameters to send
        params: serde_json::Value,

        /// Regular expressions that should apply to the resulting JSON value.
        /// If `Ok`, then success is expected.
        /// Otherwise, `Err`.
        expected: Result<Filter, Filter>,
    },

    /// Return a JSON value as the script result
    Return {
        value: serde_json::Value,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Filter {
    /// Regular expression that should apply to the result JSON value
    regex: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use tokio::sync::oneshot;

    /// Mock environment for testing - stores sampling requests and provides canned responses
    struct MockEnvironment {
        expected_requests: Vec<serde_json::Value>,
        responses: Vec<Result<serde_json::Value, serde_json::Value>>,
        request_index: std::sync::Arc<std::sync::Mutex<usize>>,
    }

    impl MockEnvironment {
        fn new() -> Self {
            Self {
                expected_requests: Vec::new(),
                responses: Vec::new(),
                request_index: std::sync::Arc::new(std::sync::Mutex::new(0)),
            }
        }

        fn expect_request(mut self, request: serde_json::Value) -> Self {
            self.expected_requests.push(request);
            self
        }

        fn respond_with(mut self, response: Result<serde_json::Value, serde_json::Value>) -> Self {
            self.responses.push(response);
            self
        }
    }

    impl ScriptEnvironment for MockEnvironment {
        fn send_sampling_request(
            &self,
            params: serde_json::Value,
            response_tx: oneshot::Sender<Result<serde_json::Value, serde_json::Value>>,
        ) -> eyre::Result<()> {
            let mut index = self.request_index.lock().unwrap();
            
            // Verify request matches expected
            if *index < self.expected_requests.len() {
                assert_eq!(params, self.expected_requests[*index], "Request {} doesn't match expected", *index);
            }
            
            // Send response if available
            if *index < self.responses.len() {
                let response = self.responses[*index].clone();
                let _ = response_tx.send(response);
            }
            
            *index += 1;
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_simple_return_script() {
        let script_json = serde_json::json!({
            "scripts": [
                {
                    "name": "hello",
                    "arguments": [],
                    "steps": [
                        {
                            "Return": {
                                "value": "Hello, World!"
                            }
                        }
                    ]
                }
            ]
        });

        let scripts: Scripts = serde_json::from_value(script_json).unwrap();
        let env = MockEnvironment::new();
        
        let result = scripts.execute_if_exists("hello", BTreeMap::new(), &env).await.unwrap();
        assert_eq!(result, Some(serde_json::json!("Hello, World!")));
    }

    #[tokio::test]
    async fn test_argument_substitution() {
        let script_json = serde_json::json!({
            "scripts": [
                {
                    "name": "greet",
                    "arguments": ["name"],
                    "steps": [
                        {
                            "Return": {
                                "value": "Hello, ${name}!"
                            }
                        }
                    ]
                }
            ]
        });

        let scripts: Scripts = serde_json::from_value(script_json).unwrap();
        let env = MockEnvironment::new();
        
        let mut args = BTreeMap::new();
        args.insert("name".to_string(), serde_json::json!("Alice"));
        
        let result = scripts.execute_if_exists("greet", args, &env).await.unwrap();
        assert_eq!(result, Some(serde_json::json!("Hello, Alice!")));
    }

    #[tokio::test]
    async fn test_sampling_step() {
        let script_json = serde_json::json!({
            "scripts": [
                {
                    "name": "ask_llm",
                    "arguments": [],
                    "steps": [
                        {
                            "Sampling": {
                                "params": {
                                    "messages": [{"role": "user", "content": "Hello"}]
                                },
                                "expected": {
                                    "Ok": {
                                        "regex": ".*greeting.*"
                                    }
                                }
                            }
                        }
                    ]
                }
            ]
        });

        let scripts: Scripts = serde_json::from_value(script_json).unwrap();
        let env = MockEnvironment::new()
            .expect_request(serde_json::json!({
                "messages": [{"role": "user", "content": "Hello"}]
            }))
            .respond_with(Ok(serde_json::json!("This is a greeting response")));
        
        let result = scripts.execute_if_exists("ask_llm", BTreeMap::new(), &env).await.unwrap();
        assert_eq!(result, None); // Sampling steps don't return values
    }

    #[tokio::test]
    async fn test_script_call() {
        let script_json = serde_json::json!({
            "scripts": [
                {
                    "name": "main",
                    "arguments": [],
                    "steps": [
                        {
                            "Call": {
                                "name": "helper",
                                "arguments": {
                                    "value": "test"
                                }
                            }
                        }
                    ]
                },
                {
                    "name": "helper",
                    "arguments": ["value"],
                    "steps": [
                        {
                            "Return": {
                                "value": "Processed: ${value}"
                            }
                        }
                    ]
                }
            ]
        });

        let scripts: Scripts = serde_json::from_value(script_json).unwrap();
        let env = MockEnvironment::new();
        
        let result = scripts.execute_if_exists("main", BTreeMap::new(), &env).await.unwrap();
        assert_eq!(result, Some(serde_json::json!("Processed: test")));
    }
}
