use std::collections::HashMap;

use serde::{
    Deserialize,
    Serialize,
};

use crate::util::command_frontmatter::{
    CommandFrontmatter,
    ParameterType,
};
use crate::util::command_types::CommandError;

/// Template metadata for command creation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandTemplate {
    /// Template identifier
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Template description
    pub description: String,
    /// Template category
    pub category: String,
    /// Template tags
    pub tags: Vec<String>,
    /// Template variables that can be customized
    pub variables: Vec<TemplateVariable>,
    /// Base frontmatter configuration
    pub frontmatter: CommandFrontmatter,
    /// Template content with placeholders
    pub content: String,
}

/// Template variable definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateVariable {
    /// Variable name (used in {{variable_name}} placeholders)
    pub name: String,
    /// Variable description
    pub description: String,
    /// Variable type
    pub var_type: ParameterType,
    /// Whether this variable is required
    pub required: bool,
    /// Default value if not provided
    pub default: Option<String>,
    /// Allowed values (for enum-like variables)
    pub options: Option<Vec<String>>,
}

/// Values provided for template instantiation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateValues {
    /// Variable name to value mapping
    pub variables: HashMap<String, String>,
    /// Override frontmatter values
    pub frontmatter_overrides: Option<HashMap<String, serde_yaml::Value>>,
}

/// Template instantiation result
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct InstantiatedTemplate {
    /// Generated frontmatter
    pub frontmatter: CommandFrontmatter,
    /// Generated content with variables replaced
    pub content: String,
}

impl CommandTemplate {
    /// Create a new command template
    pub fn new(
        id: String,
        name: String,
        description: String,
        category: String,
        variables: Vec<TemplateVariable>,
        frontmatter: CommandFrontmatter,
        content: String,
    ) -> Self {
        Self {
            id,
            name,
            description,
            category,
            tags: Vec::new(),
            variables,
            frontmatter,
            content,
        }
    }

    /// Add tags to the template
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Validate template variables against provided values
    #[allow(dead_code)]
    pub fn validate_values(&self, values: &TemplateValues) -> Result<(), CommandError> {
        // Check required variables
        for variable in &self.variables {
            if variable.required && !values.variables.contains_key(&variable.name) {
                return Err(CommandError::TemplateError(format!(
                    "Required template variable '{}' not provided",
                    variable.name
                )));
            }
        }

        // Validate variable types and options
        for (name, value) in &values.variables {
            if let Some(variable) = self.variables.iter().find(|v| v.name == *name) {
                // Check options if specified
                if let Some(options) = &variable.options {
                    if !options.contains(value) {
                        return Err(CommandError::TemplateError(format!(
                            "Invalid value '{}' for variable '{}'. Allowed values: {:?}",
                            value, name, options
                        )));
                    }
                }

                // Basic type validation
                match variable.var_type {
                    ParameterType::Integer => {
                        if value.parse::<i64>().is_err() {
                            return Err(CommandError::TemplateError(format!(
                                "Variable '{}' must be an integer, got '{}'",
                                name, value
                            )));
                        }
                    },
                    ParameterType::Float => {
                        if value.parse::<f64>().is_err() {
                            return Err(CommandError::TemplateError(format!(
                                "Variable '{}' must be a float, got '{}'",
                                name, value
                            )));
                        }
                    },
                    ParameterType::Boolean => {
                        if !matches!(
                            value.to_lowercase().as_str(),
                            "true" | "false" | "1" | "0" | "yes" | "no"
                        ) {
                            return Err(CommandError::TemplateError(format!(
                                "Variable '{}' must be a boolean, got '{}'",
                                name, value
                            )));
                        }
                    },
                    _ => {}, // String, Array, Object - accept as-is
                }
            }
        }

        Ok(())
    }

    /// Instantiate the template with provided values
    #[allow(dead_code)]
    pub fn instantiate(&self, values: &TemplateValues) -> Result<InstantiatedTemplate, CommandError> {
        self.validate_values(values)?;

        // Prepare variable substitutions
        let mut substitutions = HashMap::new();

        // Add provided values
        for (name, value) in &values.variables {
            substitutions.insert(format!("{{{{{}}}}}", name), value.clone());
        }

        // Add default values for missing optional variables
        for variable in &self.variables {
            let placeholder = format!("{{{{{}}}}}", variable.name);
            if let std::collections::hash_map::Entry::Vacant(e) = substitutions.entry(placeholder) {
                if let Some(default) = &variable.default {
                    e.insert(default.clone());
                }
            }
        }

        // Replace placeholders in content
        let mut content = self.content.clone();
        for (placeholder, value) in &substitutions {
            content = content.replace(placeholder, value);
        }

        // Create frontmatter with placeholders replaced
        let mut frontmatter = self.frontmatter.clone();

        // Replace placeholders in frontmatter fields
        if let Some(name) = &frontmatter.name {
            let mut new_name = name.clone();
            for (placeholder, value) in &substitutions {
                new_name = new_name.replace(placeholder, value);
            }
            frontmatter.name = Some(new_name);
        }

        if let Some(description) = &frontmatter.description {
            let mut new_description = description.clone();
            for (placeholder, value) in &substitutions {
                new_description = new_description.replace(placeholder, value);
            }
            frontmatter.description = Some(new_description);
        }

        if let Some(category) = &frontmatter.category {
            let mut new_category = category.clone();
            for (placeholder, value) in &substitutions {
                new_category = new_category.replace(placeholder, value);
            }
            frontmatter.category = Some(new_category);
        }

        // Replace placeholders in tags
        let mut new_tags = Vec::new();
        for tag in &frontmatter.tags {
            let mut new_tag = tag.clone();
            for (placeholder, value) in &substitutions {
                new_tag = new_tag.replace(placeholder, value);
            }
            new_tags.push(new_tag);
        }
        frontmatter.tags = new_tags;

        // Replace placeholders in allowed_tools
        let mut new_tools = Vec::new();
        for tool in &frontmatter.allowed_tools {
            let mut new_tool = tool.clone();
            for (placeholder, value) in &substitutions {
                new_tool = new_tool.replace(placeholder, value);
            }
            new_tools.push(new_tool);
        }
        frontmatter.allowed_tools = new_tools;

        // Apply frontmatter overrides after placeholder replacement
        if let Some(overrides) = &values.frontmatter_overrides {
            // Apply frontmatter overrides
            // This is a simplified implementation - in practice, you'd want more sophisticated merging
            for (key, value) in overrides {
                match key.as_str() {
                    "name" => {
                        if let Some(name) = value.as_str() {
                            frontmatter.name = Some(name.to_string());
                        }
                    },
                    "description" => {
                        if let Some(desc) = value.as_str() {
                            frontmatter.description = Some(desc.to_string());
                        }
                    },
                    "category" => {
                        if let Some(cat) = value.as_str() {
                            frontmatter.category = Some(cat.to_string());
                        }
                    },
                    _ => {}, // Ignore unknown overrides for now
                }
            }
        }

        Ok(InstantiatedTemplate { frontmatter, content })
    }

    /// Get template summary for listing
    #[allow(dead_code)]
    pub fn summary(&self) -> TemplateSummary {
        TemplateSummary {
            id: self.id.clone(),
            name: self.name.clone(),
            description: self.description.clone(),
            category: self.category.clone(),
            tags: self.tags.clone(),
            variable_count: self.variables.len(),
        }
    }
}

/// Template summary for listing operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub tags: Vec<String>,
    pub variable_count: usize,
}

impl TemplateVariable {
    /// Create a new template variable
    pub fn new(name: String, description: String, var_type: ParameterType, required: bool) -> Self {
        Self {
            name,
            description,
            var_type,
            required,
            default: None,
            options: None,
        }
    }

    /// Set default value
    pub fn with_default(mut self, default: String) -> Self {
        self.default = Some(default);
        self
    }

    /// Set allowed options
    pub fn with_options(mut self, options: Vec<String>) -> Self {
        self.options = Some(options);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::command_frontmatter::{
        Parameter,
        ParameterType,
    };

    fn create_test_template() -> CommandTemplate {
        let frontmatter = CommandFrontmatter {
            name: Some("{{command_name}}".to_string()),
            description: Some("{{command_description}}".to_string()),
            version: Some("1.0".to_string()),
            author: None,
            category: Some("{{category}}".to_string()),
            tags: vec!["template".to_string()],
            allowed_tools: vec!["execute_bash".to_string()],
            thinking_mode: Some(true),
            timeout: Some(30),
            parameters: vec![Parameter {
                name: "action".to_string(),
                param_type: ParameterType::String,
                description: Some("Action to perform".to_string()),
                required: true,
                default: None,
                options: Some(vec![
                    serde_json::Value::String("start".to_string()),
                    serde_json::Value::String("stop".to_string()),
                ]),
                validation: None,
            }],
            additional: std::collections::HashMap::new(),
        };

        CommandTemplate::new(
            "test-template".to_string(),
            "Test Template".to_string(),
            "A test template".to_string(),
            "testing".to_string(),
            vec![
                TemplateVariable::new(
                    "command_name".to_string(),
                    "Name of the command".to_string(),
                    ParameterType::String,
                    true,
                ),
                TemplateVariable::new(
                    "command_description".to_string(),
                    "Description of the command".to_string(),
                    ParameterType::String,
                    false,
                )
                .with_default("Default description".to_string()),
                TemplateVariable::new(
                    "category".to_string(),
                    "Command category".to_string(),
                    ParameterType::String,
                    false,
                )
                .with_default("general".to_string())
                .with_options(vec![
                    "general".to_string(),
                    "development".to_string(),
                    "testing".to_string(),
                ]),
            ],
            frontmatter,
            "This is a {{command_name}} command that does {{command_description}}.".to_string(),
        )
    }

    #[test]
    fn test_template_validation_success() {
        let template = create_test_template();
        let mut values = TemplateValues {
            variables: HashMap::new(),
            frontmatter_overrides: None,
        };
        values
            .variables
            .insert("command_name".to_string(), "My Command".to_string());
        values
            .variables
            .insert("category".to_string(), "development".to_string());

        assert!(template.validate_values(&values).is_ok());
    }

    #[test]
    fn test_template_validation_missing_required() {
        let template = create_test_template();
        let values = TemplateValues {
            variables: HashMap::new(),
            frontmatter_overrides: None,
        };

        let result = template.validate_values(&values);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Required template variable 'command_name'")
        );
    }

    #[test]
    fn test_template_validation_invalid_option() {
        let template = create_test_template();
        let mut values = TemplateValues {
            variables: HashMap::new(),
            frontmatter_overrides: None,
        };
        values
            .variables
            .insert("command_name".to_string(), "My Command".to_string());
        values
            .variables
            .insert("category".to_string(), "invalid_category".to_string());

        let result = template.validate_values(&values);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid value 'invalid_category'")
        );
    }

    #[test]
    fn test_template_instantiation() {
        let template = create_test_template();
        let mut values = TemplateValues {
            variables: HashMap::new(),
            frontmatter_overrides: None,
        };
        values
            .variables
            .insert("command_name".to_string(), "My Command".to_string());

        let result = template.instantiate(&values).unwrap();
        assert_eq!(result.frontmatter.name, Some("My Command".to_string()));
        assert_eq!(result.frontmatter.description, Some("Default description".to_string()));
        assert_eq!(result.frontmatter.category, Some("general".to_string()));
        assert!(result.content.contains("My Command"));
        assert!(result.content.contains("Default description"));
    }
}
