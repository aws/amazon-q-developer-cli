use std::collections::{
    BTreeMap,
    HashMap,
    HashSet,
};

use regex::Regex;

use super::agent_config::parse::CanonicalToolName;
use super::agent_loop::types::ToolSpec;
use super::consts::{
    MAX_TOOL_NAME_LEN,
    MAX_TOOL_SPEC_DESCRIPTION_LEN,
    RTS_VALID_TOOL_NAME_REGEX,
    TOOL_USE_PURPOSE_FIELD_DESCRIPTION,
    TOOL_USE_PURPOSE_FIELD_NAME,
};
use super::tools::BuiltInTool;

/// Categorizes different types of tool name validation failures according to the requirements by
/// the RTS API.
#[derive(Debug, Clone)]
#[allow(dead_code)] // TODO
pub struct ToolValidationError {
    mcp_server_name: String,
    tool_spec: ToolSpec,
    kind: ToolValidationErrorKind,
}

impl ToolValidationError {
    pub fn new(mcp_server_name: String, tool_spec: ToolSpec, kind: ToolValidationErrorKind) -> Self {
        Self {
            mcp_server_name,
            tool_spec,
            kind,
        }
    }
}

// TODO - remove dead code. Keeping for debug purposes
#[derive(Debug, Clone)]
pub enum ToolValidationErrorKind {
    OutOfSpecName {
        #[allow(dead_code)]
        transformed_name: String,
    },
    EmptyName,
    NameTooLong,
    EmptyDescription,
    DescriptionTooLong,
    NameCollision(#[allow(dead_code)] CanonicalToolName),
}

/// Represents a set of tool specs that conforms to backend validations.
///
/// # Background
///
/// MCP servers can return invalid tool specifications according to certain backend validations
/// (e.g., tool names too long, invalid name format, empty tool description, and so on).
///
/// Therefore, we need to perform some transformations on the tool name and resulting tool spec
/// before sending it to the backend.
#[derive(Debug, Clone)]
pub struct SanitizedToolSpecs {
    tool_map: HashMap<String, SanitizedToolSpec>,
    filtered_specs: Vec<ToolValidationError>,
    transformed_tool_specs: Vec<ToolValidationError>,
}

impl SanitizedToolSpecs {
    /// Mapping from a transformed tool name to the canonical tool name and corresponding tool
    /// spec.
    pub fn tool_map(&self) -> &HashMap<String, SanitizedToolSpec> {
        &self.tool_map
    }

    /// Tool specs that could not be included due to failed validations.
    pub fn filtered_specs(&self) -> &[ToolValidationError] {
        &self.filtered_specs
    }

    /// Tool specs that are included in [Self::tool_map] but underwent transformations in order to
    /// conform to the validation requirements.
    pub fn transformed_tool_specs(&self) -> &[ToolValidationError] {
        &self.transformed_tool_specs
    }
}

impl SanitizedToolSpecs {
    /// Returns a list of valid tool specs to send to the model.
    ///
    /// These tool specs are "sanitized", meaning they *should not* cause validation errors.
    pub fn tool_specs(&self) -> Vec<ToolSpec> {
        self.tool_map.values().map(|v| v.tool_spec.clone()).collect()
    }
}

/// Represents a tool spec that conforms to the backend validations.
///
/// See [SanitizedToolSpecs] for more background.
#[derive(Debug, Clone)]
pub struct SanitizedToolSpec {
    canonical_name: CanonicalToolName,
    tool_spec: ToolSpec,
}

impl SanitizedToolSpec {
    pub fn canonical_name(&self) -> &CanonicalToolName {
        &self.canonical_name
    }
}

/// Creates a set of tool specs to send to the model.
///
/// This function:
/// - Transforms invalid tool specs from MCP servers, if required and able to
/// - Resolves tool name aliases
///
/// # Arguments
///
/// - `canonical_names` - List of tool names to include in the generated tool specs
/// - `mcp_tool_specs` - Map from an MCP server name to a list of tool specs as returned by the
///   server
/// - `aliases` - Map from a canonical tool name to an aliased name. This refers to the `aliases`
///   field in the agent config
pub fn sanitize_tool_specs(
    canonical_names: Vec<CanonicalToolName>,
    mcp_tool_specs: HashMap<String, Vec<ToolSpec>>,
    aliases: &HashMap<String, String>,
) -> SanitizedToolSpecs {
    // Mapping from tool names as presented to the model, to a sanitized tool spec that won't cause
    // validation errors.
    let mut tool_map = HashMap::new();

    // Tool names for mcp servers.
    // Use a BTreeMap to ensure we process MCP servers in a deterministic order.
    let mut mcp_tool_names = BTreeMap::new();

    for name in canonical_names {
        match &name {
            canon_name @ CanonicalToolName::BuiltIn(name) => {
                tool_map.insert(name.as_ref().to_string(), SanitizedToolSpec {
                    canonical_name: canon_name.clone(),
                    tool_spec: BuiltInTool::generate_tool_spec(name),
                });
            },
            CanonicalToolName::Mcp { server_name, tool_name } => {
                // MCP tools will be processed below
                mcp_tool_names
                    .entry(server_name.clone())
                    .or_insert_with(HashSet::new)
                    .insert(tool_name.clone());
            },
            CanonicalToolName::Agent { .. } => {
                // TODO: generate tool spec from agent config
            },
        }
    }

    // Then, add each server's tools, filtering only the tools that are requested.
    let mut filtered_specs = Vec::new();
    let mut warnings = Vec::new();
    let tool_name_regex = Regex::new(RTS_VALID_TOOL_NAME_REGEX).expect("should compile");
    for (server_name, tool_names) in mcp_tool_names {
        let Some(all_tool_specs) = mcp_tool_specs.get(&server_name) else {
            continue;
        };

        let mut tool_specs = all_tool_specs.clone();
        tool_specs.retain(|t| tool_names.contains(&t.name));

        // Process MCP tool names to conform to the backend API requirements.
        //
        // Tools are subjected to the following validations:
        // 1. ^[a-zA-Z][a-zA-Z0-9_]*$,
        // 2. less than 64 characters in length
        // 3. a non-empty description
        for mut spec in tool_specs {
            let canonical_name = CanonicalToolName::from_mcp_parts(server_name.clone(), spec.name.clone());
            let full_name = canonical_name.as_full_name();
            let mut is_regex_mismatch = false;

            // First, resolve alias if exists.
            let name = aliases.get(full_name.as_ref()).cloned().unwrap_or(spec.name.clone());

            // Then, sanitize if required.
            let sanitized_name = if !tool_name_regex.is_match(&name) {
                is_regex_mismatch = true;
                name.chars()
                    .filter(|c| c.is_ascii_alphabetic() || c.is_ascii_digit() || *c == '_' || *c == '-')
                    .collect::<String>()
            } else {
                name
            };
            // Ensure first char is alphabetic.
            let sanitized_name = match sanitized_name.chars().next() {
                Some(c) if c.is_ascii_alphabetic() => sanitized_name,
                Some(_) => format!("a{}", sanitized_name),
                _ => {
                    filtered_specs.push(ToolValidationError::new(
                        server_name.clone(),
                        spec.clone(),
                        ToolValidationErrorKind::EmptyName,
                    ));
                    continue;
                },
            };

            // Perform final validations against the sanitized name.
            if sanitized_name.len() > MAX_TOOL_NAME_LEN {
                filtered_specs.push(ToolValidationError::new(
                    server_name.clone(),
                    spec.clone(),
                    ToolValidationErrorKind::NameTooLong,
                ));
            } else if spec.description.is_empty() {
                filtered_specs.push(ToolValidationError::new(
                    server_name.clone(),
                    spec.clone(),
                    ToolValidationErrorKind::EmptyDescription,
                ));
            } else if let Some(n) = tool_map.get(sanitized_name.as_str()) {
                filtered_specs.push(ToolValidationError::new(
                    server_name.clone(),
                    spec.clone(),
                    ToolValidationErrorKind::NameCollision(n.canonical_name.clone()),
                ));
            } else {
                if spec.description.len() > MAX_TOOL_SPEC_DESCRIPTION_LEN {
                    warnings.push(ToolValidationError::new(
                        server_name.clone(),
                        spec.clone(),
                        ToolValidationErrorKind::DescriptionTooLong,
                    ));
                }
                if is_regex_mismatch {
                    warnings.push(ToolValidationError::new(
                        server_name.clone(),
                        spec.clone(),
                        ToolValidationErrorKind::OutOfSpecName {
                            transformed_name: sanitized_name.clone(),
                        },
                    ));
                }
                spec.name = sanitized_name.clone();
                spec.description.truncate(MAX_TOOL_SPEC_DESCRIPTION_LEN);
                tool_map.insert(sanitized_name, SanitizedToolSpec {
                    canonical_name,
                    tool_spec: spec,
                });
            }
        }
    }

    SanitizedToolSpecs {
        tool_map,
        filtered_specs,
        transformed_tool_specs: warnings,
    }
}

/// Adds an argument to each tool spec called [TOOL_USE_PURPOSE_FIELD_NAME] in order for the model
/// to provide extra context why the tool use is being made.
pub fn add_tool_use_purpose_arg(tool_specs: &mut Vec<ToolSpec>) {
    for spec in tool_specs {
        let Some(arg_type) = spec.input_schema.get("type").and_then(|v| v.as_str()) else {
            continue;
        };
        if arg_type != "object" {
            continue;
        }
        let Some(properties) = spec.input_schema.get_mut("properties").and_then(|p| p.as_object_mut()) else {
            continue;
        };
        if !properties.contains_key(TOOL_USE_PURPOSE_FIELD_NAME) {
            let obj = serde_json::Value::Object(
                [
                    (
                        "description".to_string(),
                        serde_json::Value::String(TOOL_USE_PURPOSE_FIELD_DESCRIPTION.to_string()),
                    ),
                    ("type".to_string(), serde_json::Value::String("string".to_string())),
                ]
                .into_iter()
                .collect::<serde_json::Map<_, _>>(),
            );
            properties.insert(TOOL_USE_PURPOSE_FIELD_NAME.to_string(), obj);
        }
    }
}

// pub fn parse_tool() -> Result<Tool,
