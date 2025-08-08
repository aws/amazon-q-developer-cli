use std::collections::HashMap;
use std::fs;
use std::path::{
    Path,
    PathBuf,
};

use crate::util::command_templates::{
    CommandTemplate,
    InstantiatedTemplate,
    TemplateSummary,
    TemplateValues,
};
use crate::util::command_types::CommandError;

/// Registry for managing command templates
#[derive(Debug, Clone)]
pub struct TemplateRegistry {
    /// Built-in templates
    builtin_templates: HashMap<String, CommandTemplate>,
    /// User-defined templates
    user_templates: HashMap<String, CommandTemplate>,
    /// Template directories
    template_dirs: Vec<PathBuf>,
}

impl TemplateRegistry {
    /// Create a new template registry
    pub fn new() -> Self {
        Self {
            builtin_templates: HashMap::new(),
            user_templates: HashMap::new(),
            template_dirs: Vec::new(),
        }
    }

    /// Initialize with default template directories
    pub fn with_default_dirs() -> Result<Self, CommandError> {
        let mut registry = Self::new();

        // Add built-in templates directory
        if let Some(home_dir) = dirs::home_dir() {
            let builtin_dir = home_dir.join(".amazonq").join("templates");
            registry.add_template_dir(builtin_dir);
        }

        // Add current directory templates
        registry.add_template_dir(PathBuf::from(".amazonq/templates"));

        registry.load_builtin_templates()?;
        registry.load_user_templates()?;

        Ok(registry)
    }

    /// Add a template directory to search
    pub fn add_template_dir<P: AsRef<Path>>(&mut self, dir: P) {
        self.template_dirs.push(dir.as_ref().to_path_buf());
    }

    /// Load built-in templates
    pub fn load_builtin_templates(&mut self) -> Result<(), CommandError> {
        // For now, create some built-in templates programmatically
        // In a real implementation, these would be loaded from files
        self.builtin_templates
            .insert("git-workflow".to_string(), Self::create_git_workflow_template());

        self.builtin_templates
            .insert("aws-cli".to_string(), Self::create_aws_cli_template());

        self.builtin_templates
            .insert("basic-command".to_string(), Self::create_basic_command_template());

        Ok(())
    }

    /// Load user-defined templates from directories
    pub fn load_user_templates(&mut self) -> Result<(), CommandError> {
        let dirs = self.template_dirs.clone();
        for dir in &dirs {
            if dir.exists() && dir.is_dir() {
                self.load_templates_from_dir(dir)?;
            }
        }
        Ok(())
    }

    /// Load templates from a specific directory
    fn load_templates_from_dir(&mut self, dir: &Path) -> Result<(), CommandError> {
        let entries = fs::read_dir(dir).map_err(|e| {
            CommandError::FileError(format!("Failed to read template directory {}: {}", dir.display(), e))
        })?;

        for entry in entries {
            let entry = entry.map_err(|e| CommandError::FileError(format!("Failed to read directory entry: {}", e)))?;

            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("yaml")
                || path.extension().and_then(|s| s.to_str()) == Some("yml")
            {
                match Self::load_template_from_file(&path) {
                    Ok(template) => {
                        self.user_templates.insert(template.id.clone(), template);
                    },
                    Err(e) => {
                        eprintln!("Warning: Failed to load template from {}: {}", path.display(), e);
                    },
                }
            }
        }

        Ok(())
    }

    /// Load a template from a YAML file
    fn load_template_from_file(path: &Path) -> Result<CommandTemplate, CommandError> {
        let content = fs::read_to_string(path)
            .map_err(|e| CommandError::FileError(format!("Failed to read template file {}: {}", path.display(), e)))?;

        let template: CommandTemplate = serde_yaml::from_str(&content).map_err(|e| {
            CommandError::ParseError(format!("Failed to parse template file {}: {}", path.display(), e))
        })?;

        Ok(template)
    }

    /// Get a template by ID
    #[allow(dead_code)]
    pub fn get_template(&self, id: &str) -> Option<&CommandTemplate> {
        self.builtin_templates.get(id).or_else(|| self.user_templates.get(id))
    }

    /// List all available templates
    #[allow(dead_code)]
    pub fn list_templates(&self) -> Vec<TemplateSummary> {
        let mut templates = Vec::new();

        // Add built-in templates
        for template in self.builtin_templates.values() {
            templates.push(template.summary());
        }

        // Add user templates
        for template in self.user_templates.values() {
            templates.push(template.summary());
        }

        templates.sort_by(|a, b| a.name.cmp(&b.name));
        templates
    }

    /// List templates by category
    #[allow(dead_code)]
    pub fn list_templates_by_category(&self, category: &str) -> Vec<TemplateSummary> {
        self.list_templates()
            .into_iter()
            .filter(|t| t.category == category)
            .collect()
    }

    /// Search templates by name or description
    #[allow(dead_code)]
    pub fn search_templates(&self, query: &str) -> Vec<TemplateSummary> {
        let query_lower = query.to_lowercase();
        self.list_templates()
            .into_iter()
            .filter(|t| {
                t.name.to_lowercase().contains(&query_lower)
                    || t.description.to_lowercase().contains(&query_lower)
                    || t.tags.iter().any(|tag| tag.to_lowercase().contains(&query_lower))
            })
            .collect()
    }

    /// Instantiate a template with values
    #[allow(dead_code)]
    pub fn instantiate_template(
        &self,
        id: &str,
        values: &TemplateValues,
    ) -> Result<InstantiatedTemplate, CommandError> {
        let template = self
            .get_template(id)
            .ok_or_else(|| CommandError::TemplateError(format!("Template '{}' not found", id)))?;

        template.instantiate(values)
    }

    /// Get all categories
    #[allow(dead_code)]
    pub fn get_categories(&self) -> Vec<String> {
        let mut categories = std::collections::HashSet::new();

        for template in self.builtin_templates.values().chain(self.user_templates.values()) {
            categories.insert(template.category.clone());
        }

        let mut result: Vec<String> = categories.into_iter().collect();
        result.sort();
        result
    }

    // Built-in template creators
    fn create_git_workflow_template() -> CommandTemplate {
        use crate::util::command_frontmatter::{
            CommandFrontmatter,
            Parameter,
            ParameterType,
        };
        use crate::util::command_templates::{
            CommandTemplate,
            TemplateVariable,
        };

        let frontmatter = CommandFrontmatter {
            name: Some("{{workflow_name}}".to_string()),
            description: Some("Git workflow automation for {{workflow_type}}".to_string()),
            version: Some("1.0".to_string()),
            author: None,
            category: Some("development".to_string()),
            tags: vec!["git".to_string(), "workflow".to_string(), "automation".to_string()],
            allowed_tools: vec!["execute_bash".to_string(), "fs_read".to_string()],
            thinking_mode: Some(true),
            timeout: Some(60),
            parameters: vec![
                Parameter {
                    name: "action".to_string(),
                    param_type: ParameterType::String,
                    description: Some("Git action to perform".to_string()),
                    required: true,
                    default: None,
                    options: Some(vec![
                        serde_json::Value::String("create-branch".to_string()),
                        serde_json::Value::String("merge".to_string()),
                        serde_json::Value::String("rebase".to_string()),
                        serde_json::Value::String("status".to_string()),
                    ]),
                    validation: None,
                },
                Parameter {
                    name: "branch".to_string(),
                    param_type: ParameterType::String,
                    description: Some("Branch name".to_string()),
                    required: false,
                    default: Some(serde_json::Value::String("{{default_branch}}".to_string())),
                    options: None,
                    validation: None,
                },
            ],
            additional: std::collections::HashMap::new(),
        };

        CommandTemplate::new(
            "git-workflow".to_string(),
            "Git Workflow".to_string(),
            "Automated git workflow operations".to_string(),
            "development".to_string(),
            vec![
                TemplateVariable::new(
                    "workflow_name".to_string(),
                    "Name of the git workflow command".to_string(),
                    ParameterType::String,
                    true,
                ),
                TemplateVariable::new(
                    "workflow_type".to_string(),
                    "Type of workflow (feature, hotfix, release)".to_string(),
                    ParameterType::String,
                    false,
                )
                .with_default("feature".to_string())
                .with_options(vec!["feature".to_string(), "hotfix".to_string(), "release".to_string()]),
                TemplateVariable::new(
                    "default_branch".to_string(),
                    "Default branch name".to_string(),
                    ParameterType::String,
                    false,
                )
                .with_default("main".to_string()),
            ],
            frontmatter,
            r#"# {{workflow_name}}

This command automates {{workflow_type}} workflow operations.

## Usage

Use this command to manage your {{workflow_type}} workflow:

- Create new branches
- Merge changes
- Check status
- Perform rebases

The default branch is set to `{{default_branch}}`.

## Implementation

Based on the action parameter, this command will:

1. **create-branch**: Create a new {{workflow_type}} branch
2. **merge**: Merge changes to {{default_branch}}
3. **rebase**: Rebase current branch on {{default_branch}}
4. **status**: Show current git status

Execute git commands using the execute_bash tool with appropriate error handling.
"#
            .to_string(),
        )
        .with_tags(vec![
            "git".to_string(),
            "workflow".to_string(),
            "development".to_string(),
        ])
    }

    fn create_aws_cli_template() -> CommandTemplate {
        use crate::util::command_frontmatter::{
            CommandFrontmatter,
            Parameter,
            ParameterType,
        };
        use crate::util::command_templates::{
            CommandTemplate,
            TemplateVariable,
        };

        let frontmatter = CommandFrontmatter {
            name: Some("{{service_name}} Helper".to_string()),
            description: Some("AWS {{service_name}} operations helper".to_string()),
            version: Some("1.0".to_string()),
            author: None,
            category: Some("aws".to_string()),
            tags: vec!["aws".to_string(), "{{service_name}}".to_string(), "cloud".to_string()],
            allowed_tools: vec!["use_aws".to_string(), "fs_read".to_string()],
            thinking_mode: Some(true),
            timeout: Some(120),
            parameters: vec![
                Parameter {
                    name: "operation".to_string(),
                    param_type: ParameterType::String,
                    description: Some("AWS operation to perform".to_string()),
                    required: true,
                    default: None,
                    options: None,
                    validation: None,
                },
                Parameter {
                    name: "region".to_string(),
                    param_type: ParameterType::String,
                    description: Some("AWS region".to_string()),
                    required: false,
                    default: Some(serde_json::Value::String("{{default_region}}".to_string())),
                    options: None,
                    validation: None,
                },
            ],
            additional: std::collections::HashMap::new(),
        };

        CommandTemplate::new(
            "aws-cli".to_string(),
            "AWS CLI Helper".to_string(),
            "Template for AWS service operations".to_string(),
            "aws".to_string(),
            vec![
                TemplateVariable::new(
                    "service_name".to_string(),
                    "AWS service name (e.g., s3, ec2, lambda)".to_string(),
                    ParameterType::String,
                    true,
                ),
                TemplateVariable::new(
                    "default_region".to_string(),
                    "Default AWS region".to_string(),
                    ParameterType::String,
                    false,
                )
                .with_default("us-east-1".to_string()),
            ],
            frontmatter,
            r#"# {{service_name}} Helper

This command provides helper operations for AWS {{service_name}}.

## Usage

Specify the operation you want to perform and optionally the region.

Default region: {{default_region}}

## Implementation

Use the use_aws tool to make AWS CLI calls for {{service_name}} operations.
Handle errors appropriately and provide clear feedback to the user.

Example operations might include:
- List resources
- Create resources  
- Update configurations
- Delete resources
- Get status information

Always validate parameters and provide helpful error messages.
"#
            .to_string(),
        )
        .with_tags(vec!["aws".to_string(), "cloud".to_string(), "automation".to_string()])
    }

    fn create_basic_command_template() -> CommandTemplate {
        use crate::util::command_frontmatter::{
            CommandFrontmatter,
            Parameter,
            ParameterType,
        };
        use crate::util::command_templates::{
            CommandTemplate,
            TemplateVariable,
        };

        let frontmatter = CommandFrontmatter {
            name: Some("{{command_name}}".to_string()),
            description: Some("{{command_description}}".to_string()),
            version: Some("1.0".to_string()),
            author: None,
            category: Some("{{category}}".to_string()),
            tags: vec!["custom".to_string()],
            allowed_tools: vec!["{{primary_tool}}".to_string()],
            thinking_mode: Some(false),
            timeout: Some(30),
            parameters: vec![Parameter {
                name: "input".to_string(),
                param_type: ParameterType::String,
                description: Some("Input for the command".to_string()),
                required: true,
                default: None,
                options: None,
                validation: None,
            }],
            additional: std::collections::HashMap::new(),
        };

        CommandTemplate::new(
            "basic-command".to_string(),
            "Basic Command".to_string(),
            "A simple command template for getting started".to_string(),
            "general".to_string(),
            vec![
                TemplateVariable::new(
                    "command_name".to_string(),
                    "Name of your command".to_string(),
                    ParameterType::String,
                    true,
                ),
                TemplateVariable::new(
                    "command_description".to_string(),
                    "Description of what your command does".to_string(),
                    ParameterType::String,
                    true,
                ),
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
                    "aws".to_string(),
                    "automation".to_string(),
                    "utility".to_string(),
                ]),
                TemplateVariable::new(
                    "primary_tool".to_string(),
                    "Primary tool this command will use".to_string(),
                    ParameterType::String,
                    false,
                )
                .with_default("execute_bash".to_string())
                .with_options(vec![
                    "execute_bash".to_string(),
                    "fs_read".to_string(),
                    "fs_write".to_string(),
                    "use_aws".to_string(),
                ]),
            ],
            frontmatter,
            r#"# {{command_name}}

{{command_description}}

## Usage

This command takes an input parameter and processes it using {{primary_tool}}.

## Implementation

Add your command logic here. Use the available tools to:

1. Process the input parameter
2. Perform the required operations
3. Return results to the user

Remember to handle errors gracefully and provide clear feedback.
"#
            .to_string(),
        )
        .with_tags(vec!["template".to_string(), "basic".to_string()])
    }
}

impl Default for TemplateRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn test_registry_creation() {
        let registry = TemplateRegistry::new();
        assert_eq!(registry.builtin_templates.len(), 0);
        assert_eq!(registry.user_templates.len(), 0);
    }

    #[test]
    fn test_builtin_templates_loading() {
        let mut registry = TemplateRegistry::new();
        registry.load_builtin_templates().unwrap();

        assert!(!registry.builtin_templates.is_empty());
        assert!(registry.get_template("git-workflow").is_some());
        assert!(registry.get_template("aws-cli").is_some());
        assert!(registry.get_template("basic-command").is_some());
    }

    #[test]
    fn test_template_listing() {
        let mut registry = TemplateRegistry::new();
        registry.load_builtin_templates().unwrap();

        let templates = registry.list_templates();
        assert!(templates.len() >= 3);

        let git_template = templates.iter().find(|t| t.id == "git-workflow");
        assert!(git_template.is_some());
        assert_eq!(git_template.unwrap().category, "development");
    }

    #[test]
    fn test_template_search() {
        let mut registry = TemplateRegistry::new();
        registry.load_builtin_templates().unwrap();

        let results = registry.search_templates("git");
        assert!(!results.is_empty());
        assert!(results.iter().any(|t| t.id == "git-workflow"));

        let results = registry.search_templates("aws");
        assert!(results.iter().any(|t| t.id == "aws-cli"));
    }

    #[test]
    fn test_template_instantiation() {
        let mut registry = TemplateRegistry::new();
        registry.load_builtin_templates().unwrap();

        let mut values = TemplateValues {
            variables: HashMap::new(),
            frontmatter_overrides: None,
        };
        values
            .variables
            .insert("command_name".to_string(), "My Test Command".to_string());
        values
            .variables
            .insert("command_description".to_string(), "A test command".to_string());

        let result = registry.instantiate_template("basic-command", &values);
        assert!(result.is_ok());

        let instantiated = result.unwrap();
        assert_eq!(instantiated.frontmatter.name, Some("My Test Command".to_string()));
        assert!(instantiated.content.contains("My Test Command"));
    }
}
