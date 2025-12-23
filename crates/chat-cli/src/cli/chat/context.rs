use std::collections::HashMap;
use std::io::Write;
use std::path::Path;

use eyre::{
    Result,
    eyre,
};
use glob::glob;
use serde::{
    Deserialize,
    Deserializer,
    Serialize,
    Serializer,
};

use super::cli::hooks::HookOutput;
use super::cli::model::context_window_tokens;
use super::util::drop_matched_context_files;
use crate::cli::agent::Agent;
use crate::cli::agent::hook::{
    Hook,
    HookTrigger,
};
use crate::cli::chat::ChatError;
use crate::cli::chat::cli::hooks::HookExecutor;
use crate::cli::chat::cli::model::ModelInfo;
use crate::os::Os;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Inclusion {
    #[default]
    Always,
    Auto,
}

#[derive(Debug, Clone)]
pub enum ContextFilePath {
    /// Signifies that the path is brought in from the agent config
    Agent(String, Inclusion), // (path, inclusion)
    /// Signifies that the path is brought in via /context add
    Session(String),
}

#[derive(Debug, Clone)]
pub enum ContextFile {
    Full {
        filepath: String,
        content: String,
    },
    Auto {
        name: String,
        filepath: String,
        description: String,
    },
}

impl ContextFile {
    pub fn filepath(&self) -> &str {
        match self {
            ContextFile::Full { filepath, .. } | ContextFile::Auto { filepath, .. } => filepath,
        }
    }

    pub fn size(&self) -> usize {
        use crate::cli::chat::token_counter::TokenCounter;
        match self {
            ContextFile::Full { content, .. } => TokenCounter::count_tokens(content),
            ContextFile::Auto { .. } => 0,
        }
    }
}

impl Serialize for ContextFilePath {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            ContextFilePath::Agent(path, _) | ContextFilePath::Session(path) => path.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for ContextFilePath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let path = String::deserialize(deserializer)?;
        Ok(ContextFilePath::Agent(path, Inclusion::Always))
    }
}

impl ContextFilePath {
    pub fn get_path_as_str(&self) -> &str {
        match self {
            Self::Agent(path, _) | Self::Session(path) => path.as_str(),
        }
    }

    pub fn get_inclusion(&self) -> &Inclusion {
        match self {
            Self::Agent(_, inclusion) => inclusion,
            Self::Session(_) => &Inclusion::Always,
        }
    }
}

impl PartialEq for ContextFilePath {
    fn eq(&self, other: &Self) -> bool {
        let self_path = self.get_path_as_str();
        let other_path = other.get_path_as_str();

        self_path == other_path
    }
}

impl PartialEq<str> for ContextFilePath {
    fn eq(&self, other: &str) -> bool {
        self.get_path_as_str() == other
    }
}

impl PartialEq<ContextFilePath> for String {
    fn eq(&self, other: &ContextFilePath) -> bool {
        let inner = match other {
            ContextFilePath::Agent(path, _) | ContextFilePath::Session(path) => path,
        };

        self == inner
    }
}

/// Manager for context files and profiles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextManager {
    max_context_files_size: usize,
    /// Name of the current active profile.
    pub current_profile: String,
    /// List of file paths or glob patterns to include in the context.
    pub paths: Vec<ContextFilePath>,
    /// Map of Hook Name to [`Hook`]. The hook name serves as the hook's ID.
    pub hooks: HashMap<HookTrigger, Vec<Hook>>,
    #[serde(skip)]
    pub hook_executor: HookExecutor,
}

impl ContextManager {
    pub fn from_agent(agent: &Agent, max_context_files_size: usize) -> Result<Self> {
        use crate::cli::agent::wrapper_types::ResourcePath;

        let paths = agent
            .resources
            .iter()
            .filter_map(|resource| match resource {
                ResourcePath::FilePath(uri) => {
                    if !uri.starts_with("file://") {
                        return None;
                    }
                    let path = uri.trim_start_matches("file://").to_string();
                    Some(ContextFilePath::Agent(path, Inclusion::Always))
                },
                ResourcePath::Skill(uri) => {
                    if !uri.starts_with("skill://") {
                        return None;
                    }
                    let path = uri.trim_start_matches("skill://").to_string();
                    Some(ContextFilePath::Agent(path, Inclusion::Auto))
                },
                ResourcePath::Complex(_) => None,
            })
            .collect::<Vec<_>>();

        Ok(Self {
            max_context_files_size,
            current_profile: agent.name.clone(),
            paths,
            hooks: agent.hooks.clone(),
            hook_executor: HookExecutor::new(),
        })
    }

    /// Add paths to the context configuration.
    ///
    /// # Arguments
    /// * `paths` - List of paths to add
    /// * `force` - If true, skip validation that the path exists
    ///
    /// # Returns
    /// A Result indicating success or an error
    pub async fn add_paths(&mut self, os: &Os, paths: Vec<String>, force: bool) -> Result<()> {
        // Validate paths exist before adding them
        if !force {
            let mut context_files = Vec::new();

            // Check each path to make sure it exists or matches at least one file
            for path in &paths {
                // We're using a temporary context_files vector just for validation
                // Pass is_validation=true to ensure we error if glob patterns don't match any files
                match process_path(os, path, &mut context_files, true, &Inclusion::Always).await {
                    Ok(_) => {}, // Path is valid
                    Err(e) => return Err(eyre!("Invalid path '{}': {}. Use --force to add anyway.", path, e)),
                }
            }
        }

        for path in paths {
            if self.paths.iter().any(|p| p == path.as_str()) {
                return Err(eyre!("Rule '{}' already exists.", path));
            }

            // The assumption here is that we are only calling [add_paths] for adding paths in
            // session
            self.paths.push(ContextFilePath::Session(path));
        }

        Ok(())
    }

    /// Remove paths from the context configuration.
    ///
    /// # Arguments
    /// * `paths` - List of paths to remove
    ///
    /// # Returns
    /// A Result indicating success or an error
    pub fn remove_paths(&mut self, paths: Vec<String>) -> Result<()> {
        // Remove each path if it exists
        let old_path_num = self.paths.len();
        self.paths
            .retain(|p| !paths.iter().any(|path| path.as_str() == p.get_path_as_str()));

        if old_path_num == self.paths.len() {
            return Err(eyre!("None of the specified paths were found in the context"));
        }

        Ok(())
    }

    /// Clear all paths from the context configuration.
    pub fn clear(&mut self) {
        self.paths.clear();
    }

    /// Get all context files (global + profile-specific).
    ///
    /// This method:
    /// 1. Processes all paths in the global and profile configurations
    /// 2. Expands glob patterns to include matching files
    /// 3. Reads the content of each file
    /// 4. Returns a vector of (filename, content) pairs
    ///
    ///
    /// # Returns
    /// A Result containing a vector of context files or an error
    pub async fn get_context_files(&self, os: &Os) -> Result<Vec<ContextFile>> {
        let mut context_files = Vec::new();

        for path in &self.paths {
            // Use is_validation=false to handle non-matching globs gracefully
            process_path(
                os,
                path.get_path_as_str(),
                &mut context_files,
                false,
                path.get_inclusion(),
            )
            .await?;
        }

        context_files.sort_by(|a, b| a.filepath().cmp(b.filepath()));
        context_files.dedup_by(|a, b| a.filepath() == b.filepath());

        Ok(context_files)
    }

    pub async fn get_context_files_by_path(&self, os: &Os, path: &ContextFilePath) -> Result<Vec<ContextFile>> {
        let mut context_files = Vec::new();
        process_path(
            os,
            path.get_path_as_str(),
            &mut context_files,
            true,
            path.get_inclusion(),
        )
        .await?;
        Ok(context_files)
    }

    /// Collects context files and optionally drops files if the total size exceeds the limit.
    /// Returns (files_to_use, dropped_files)
    pub async fn collect_context_files_with_limit(&self, os: &Os) -> Result<(Vec<ContextFile>, Vec<ContextFile>)> {
        let mut context_files = self.get_context_files(os).await?;

        let dropped_files = drop_matched_context_files(&mut context_files, self.max_context_files_size)?;

        Ok((context_files, dropped_files))
    }

    /// Run all the currently enabled hooks from both the global and profile contexts.
    /// # Returns
    /// A vector containing pairs of a [`Hook`] definition and its execution output
    pub async fn run_hooks(
        &mut self,
        trigger: HookTrigger,
        output: &mut impl Write,
        os: &crate::os::Os,
        prompt: Option<&str>,
        tool_context: Option<crate::cli::chat::cli::hooks::ToolContext>,
    ) -> Result<Vec<((HookTrigger, Hook), HookOutput)>, ChatError> {
        let mut hooks = self.hooks.clone();
        hooks.retain(|t, _| *t == trigger);
        let cwd = os.env.current_dir()?.to_string_lossy().to_string();
        self.hook_executor
            .run_hooks(hooks, output, &cwd, prompt, tool_context)
            .await
    }
}

/// Calculates the maximum context files size to use for the given model id.
pub fn calc_max_context_files_size(model: Option<&ModelInfo>) -> usize {
    // Sets the max as 75% of the context window
    context_window_tokens(model).saturating_mul(3) / 4
}

/// Process a path, handling glob patterns and file types.
///
/// This method:
/// 1. Expands the path (handling ~ for home directory)
/// 2. If the path contains glob patterns, expands them
/// 3. For each resulting path, adds the file to the context collection
/// 4. Handles directories by including all files in the directory (non-recursive)
/// 5. With force=true, includes paths that don't exist yet
///
/// # Arguments
/// * `path` - The path to process
/// * `context_files` - The collection to add files to
/// * `is_validation` - If true, error when glob patterns don't match; if false, silently skip
///
/// # Returns
/// A Result indicating success or an error
async fn process_path(
    os: &Os,
    path: &str,
    context_files: &mut Vec<ContextFile>,
    is_validation: bool,
    inclusion: &Inclusion,
) -> Result<()> {
    // Expand ~ to home directory
    let expanded_path = if path.starts_with('~') {
        if let Some(home_dir) = os.env.home() {
            home_dir.join(&path[2..]).to_string_lossy().to_string()
        } else {
            return Err(eyre!("Could not determine home directory"));
        }
    } else {
        path.to_string()
    };

    // Handle absolute, relative paths, and glob patterns
    let full_path = if expanded_path.starts_with('/') {
        expanded_path
    } else {
        os.env.current_dir()?.join(&expanded_path).to_string_lossy().to_string()
    };

    // Required in chroot testing scenarios so that we can use `Path::exists`.
    let full_path = os.fs.chroot_path_str(full_path);

    // Check if the path contains glob patterns
    if full_path.contains('*') || full_path.contains('?') || full_path.contains('[') {
        // Expand glob pattern
        match glob(&full_path) {
            Ok(entries) => {
                let mut found_any = false;

                for entry in entries {
                    match entry {
                        Ok(path) => {
                            if path.is_file() {
                                add_file_to_context(os, &path, context_files, inclusion).await?;
                                found_any = true;
                            }
                        },
                        Err(e) => return Err(eyre!("Glob error: {}", e)),
                    }
                }

                if !found_any && is_validation {
                    // When validating paths (e.g., for /context add), error if no files match
                    return Err(eyre!("No files found matching glob pattern '{}'", full_path));
                }
                // When just showing expanded files (e.g., for /context show --expand),
                // silently skip non-matching patterns (don't add anything to context_files)
            },
            Err(e) => return Err(eyre!("Invalid glob pattern '{}': {}", full_path, e)),
        }
    } else {
        // Regular path
        let path = Path::new(&full_path);
        if path.exists() {
            if path.is_file() {
                add_file_to_context(os, path, context_files, inclusion).await?;
            } else if path.is_dir() {
                // For directories, add all files in the directory (non-recursive)
                let mut read_dir = os.fs.read_dir(path).await?;
                while let Some(entry) = read_dir.next_entry().await? {
                    let path = entry.path();
                    if path.is_file() {
                        add_file_to_context(os, &path, context_files, inclusion).await?;
                    }
                }
            }
        } else if is_validation {
            // When validating paths (e.g., for /context add), error if the path doesn't exist
            return Err(eyre!("Path '{}' does not exist", full_path));
        }
    }

    Ok(())
}

/// Add a file to the context collection.
///
/// This method:
/// 1. Reads the content of the file
/// 2. Checks front matter inclusion rules for steering files
/// 3. Adds the file to the context collection
///
/// # Arguments
/// * `path` - The path to the file
/// * `context_files` - The collection to add the file to
///
/// # Returns
/// A Result indicating success or an error
async fn add_file_to_context(
    os: &Os,
    path: &Path,
    context_files: &mut Vec<ContextFile>,
    inclusion: &Inclusion,
) -> Result<()> {
    let filename = path.to_string_lossy().to_string();
    let content = os.fs.read_to_string(path).await?;

    // Check if this is a steering file that needs front matter filtering
    if filename.contains(".kiro/steering") && filename.ends_with(".md") && !should_include_steering_file(&content)? {
        return Ok(());
    }

    if inclusion == &Inclusion::Auto {
        create_auto_load_context_file(filename, content, context_files);
    } else {
        context_files.push(ContextFile::Full {
            filepath: filename,
            content,
        });
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct FrontMatter {
    inclusion: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AutoLoadContextFileMetadata {
    name: Option<String>,
    description: Option<String>,
}

/// Extract YAML frontmatter from file content
fn extract_yaml_frontmatter(content: &str) -> Option<String> {
    if !content.starts_with("---\n") {
        return None;
    }

    let lines: Vec<&str> = content.lines().collect();
    let mut end_index = None;

    for (i, line) in lines.iter().enumerate().skip(1) {
        if line.trim() == "---" {
            end_index = Some(i);
            break;
        }
    }

    let end_index = end_index?;
    let front_matter_lines = &lines[1..end_index];
    Some(front_matter_lines.join("\n"))
}

/// Create Auto ContextFile from content, fallback to Full file
fn create_auto_load_context_file(filename: String, content: String, context_files: &mut Vec<ContextFile>) {
    if let Some(yaml) = extract_yaml_frontmatter(&content) {
        if let Ok(metadata) = parse_auto_load_metadata(&yaml) {
            context_files.push(ContextFile::Auto {
                name: metadata.name.unwrap_or_else(|| filename.clone()),
                filepath: filename,
                description: metadata
                    .description
                    .unwrap_or_else(|| "No description available".to_string()),
            });
            return;
        }
    }

    // Fallback: add as Full file
    context_files.push(ContextFile::Full {
        filepath: filename,
        content,
    });
}

/// Parse frontmatter for auto files to extract name and description
fn parse_auto_load_metadata(yaml: &str) -> Result<AutoLoadContextFileMetadata> {
    serde_yaml::from_str::<AutoLoadContextFileMetadata>(yaml).map_err(|e| eyre!("Failed to parse frontmatter: {}", e))
}

/// Check if a steering file should be included based on its front matter
fn should_include_steering_file(content: &str) -> Result<bool> {
    let front_matter_yaml = match extract_yaml_frontmatter(content) {
        Some(yaml) => yaml,
        None => return Ok(true), // No front matter - include the file
    };

    match serde_yaml::from_str::<FrontMatter>(&front_matter_yaml) {
        Ok(front_matter) => {
            match front_matter.inclusion.as_deref() {
                Some("always") => Ok(true),
                Some("fileMatch") => Ok(false), // Exclude fileMatch files
                Some("manual") => Ok(false),    // Exclude manual files
                None => Ok(true),               // No inclusion field - include
                Some(_) => Ok(true),            // Unknown inclusion value - include
            }
        },
        Err(_) => {
            // Failed to parse front matter - include the file
            Ok(true)
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::chat::util::test::create_test_context_manager;

    #[tokio::test]
    async fn test_collect_exceeds_limit() -> Result<()> {
        let os = Os::new().await.unwrap();
        let mut manager = create_test_context_manager(Some(2)).expect("Failed to create test context manager");

        os.fs.create_dir_all("test").await?;
        os.fs.write("test/to-include.md", "ha").await?;
        os.fs.write("test/to-drop.md", "long content that exceed limit").await?;
        manager.add_paths(&os, vec!["test/*.md".to_string()], false).await?;

        let (used, dropped) = manager.collect_context_files_with_limit(&os).await.unwrap();

        assert!(used.len() + dropped.len() == 2);
        assert!(used.len() == 1);
        assert!(dropped.len() == 1);
        Ok(())
    }

    #[tokio::test]
    async fn test_path_ops() -> Result<()> {
        use crate::cli::chat::context::ContextFile;

        let os = Os::new().await.unwrap();
        let mut manager = create_test_context_manager(None).expect("Failed to create test context manager");

        // Create some test files for matching.
        os.fs.create_dir_all("test").await?;
        os.fs.write("test/p1.md", "p1").await?;
        os.fs.write("test/p2.md", "p2").await?;

        assert!(
            manager.get_context_files(&os).await?.is_empty(),
            "no files should be returned for an empty profile when force is false"
        );

        manager.add_paths(&os, vec!["test/*.md".to_string()], false).await?;
        let files = manager.get_context_files(&os).await?;

        if let ContextFile::Full { filepath, content } = &files[0] {
            assert!(filepath.ends_with("p1.md"));
            assert_eq!(content, "p1");
        } else {
            panic!("Expected Full file");
        }

        if let ContextFile::Full { filepath, content } = &files[1] {
            assert!(filepath.ends_with("p2.md"));
            assert_eq!(content, "p2");
        } else {
            panic!("Expected Full file");
        }

        assert!(
            manager
                .add_paths(&os, vec!["test/*.txt".to_string()], false)
                .await
                .is_err(),
            "adding a glob with no matching and without force should fail"
        );

        Ok(())
    }

    #[test]
    fn test_calc_max_context_files_size() {
        assert_eq!(
            calc_max_context_files_size(Some(&ModelInfo {
                model_id: "CLAUDE_SONNET_4_20250514_V1_0".to_string(),
                description: None,
                model_name: Some("Claude".to_string()),
                context_window_tokens: 200_000,
                rate_multiplier: None,
                rate_unit: None,
            })),
            150_000
        );
        assert_eq!(
            calc_max_context_files_size(Some(&ModelInfo {
                model_id: "OPENAI_GPT_OSS_120B_1_0".to_string(),
                description: None,
                model_name: Some("GPT".to_string()),
                context_window_tokens: 128_000,
                rate_multiplier: None,
                rate_unit: None,
            })),
            96_000
        );
    }

    #[test]
    fn test_should_include_steering_file() {
        // Test file without front matter - should be included
        let content_no_frontmatter = "# Regular markdown file\nSome content here.";
        assert!(should_include_steering_file(content_no_frontmatter).unwrap());

        // Test file with inclusion: always - should be included
        let content_always = "---\ninclusion: always\n---\n# Always included\nContent here.";
        assert!(should_include_steering_file(content_always).unwrap());

        // Test file with inclusion: fileMatch - should be excluded
        let content_filematch = "---\ninclusion: fileMatch\n---\n# File match only\nContent here.";
        assert!(!should_include_steering_file(content_filematch).unwrap());

        // Test file with inclusion: manual - should be excluded
        let content_manual = "---\ninclusion: manual\n---\n# Manual only\nContent here.";
        assert!(!should_include_steering_file(content_manual).unwrap());

        // Test file with no inclusion field - should be included
        let content_no_inclusion = "---\ntitle: Some Title\n---\n# No inclusion field\nContent here.";
        assert!(should_include_steering_file(content_no_inclusion).unwrap());

        // Test file with malformed front matter - should be included
        let content_malformed = "---\ninvalid yaml: [\n---\n# Malformed\nContent here.";
        assert!(should_include_steering_file(content_malformed).unwrap());

        // Test file with incomplete front matter - should be included
        let content_incomplete = "---\ninclusion: always\n# Missing closing ---\nContent here.";
        assert!(should_include_steering_file(content_incomplete).unwrap());
    }

    #[test]
    fn test_mixed_resource_types_parsing() -> Result<()> {
        use serde_json;

        use crate::cli::agent::Agent;

        // Test agent config with mixed resource types
        let agent_json = r#"{
            "name": "test-agent",
            "resources": [
                "file://README.md",
                "skill://skills/**/SKILL.md"
            ]
        }"#;

        let agent: Agent = serde_json::from_str(agent_json)?;
        let context_manager = ContextManager::from_agent(&agent, 1000)?;

        // Verify we have 2 paths
        assert_eq!(context_manager.paths.len(), 2);

        // Verify the inclusion modes are correct
        let paths: Vec<_> = context_manager.paths.iter().collect();

        // First resource: "file://README.md" -> inclusion "always"
        assert_eq!(paths[0].get_path_as_str(), "README.md");
        assert_eq!(paths[0].get_inclusion(), &Inclusion::Always);

        // Second resource: "skill://skills/**/SKILL.md" -> inclusion "auto"
        assert_eq!(paths[1].get_path_as_str(), "skills/**/SKILL.md");
        assert_eq!(paths[1].get_inclusion(), &Inclusion::Auto);

        Ok(())
    }

    #[test]
    fn test_from_agent_excludes_knowledge_base() {
        use crate::cli::agent::Agent;
        use crate::cli::agent::wrapper_types::{
            ComplexResource,
            ResourcePath,
        };

        let mut agent = Agent::default();
        agent.resources = vec![
            ResourcePath::FilePath("file://src/main.rs".to_string()),
            ResourcePath::Skill("skill://src/lib.rs".to_string()),
            ResourcePath::Complex(ComplexResource::KnowledgeBase {
                source: "file://docs".to_string(),
                name: None,
                description: None,
                index_type: None,
                include: None,
                exclude: None,
                auto_update: None,
            }),
        ];

        let manager = ContextManager::from_agent(&agent, 1000).unwrap();
        assert_eq!(manager.paths.len(), 2);
        assert_eq!(manager.paths[0].get_path_as_str(), "src/main.rs");
        assert_eq!(manager.paths[0].get_inclusion(), &Inclusion::Always);
        assert_eq!(manager.paths[1].get_path_as_str(), "src/lib.rs");
        assert_eq!(manager.paths[1].get_inclusion(), &Inclusion::Auto);
    }

    #[tokio::test]
    async fn test_collect_auto_load_context_files() -> Result<()> {
        use crate::cli::agent::Agent;
        use crate::cli::agent::wrapper_types::ResourcePath;
        use crate::os::Os;

        // Create test OS and write files
        let os = Os::new().await?;

        // Regular file
        let regular_file = "README.md";
        os.fs
            .write(regular_file, "# Regular File\nThis is regular content")
            .await?;

        // Auto file with frontmatter
        let auto_file = "test-skill.md";
        let auto_content = "---\nname: database-helper\ndescription: Helps with database queries\n---\n# Database Helper\nSome content here";
        os.fs.write(auto_file, auto_content).await?;

        // Create agent with both regular and auto resources
        let mut agent = Agent::default();
        agent.name = "TestAgent".to_string();
        agent
            .resources
            .push(ResourcePath::FilePath(format!("file://{}", regular_file)));
        agent
            .resources
            .push(ResourcePath::Skill(format!("skill://{}", auto_file)));

        let context_manager = ContextManager::from_agent(&agent, 1000)?;

        // Test the method
        let (context_files, dropped_files) = context_manager.collect_context_files_with_limit(&os).await?;

        // Assertions
        assert_eq!(context_files.len(), 2);
        assert_eq!(dropped_files.len(), 0);

        // Check regular file
        if let ContextFile::Full { filepath, content } = &context_files[0] {
            assert!(filepath.contains("README.md"));
            assert!(content.contains("# Regular File"));
        } else {
            panic!("Expected Full file");
        }

        // Check auto file
        if let ContextFile::Auto {
            name,
            filepath,
            description,
        } = &context_files[1]
        {
            assert_eq!(name, "database-helper");
            assert_eq!(description, "Helps with database queries");
            assert!(filepath.contains("test-skill.md"));
        } else {
            panic!("Expected Auto file");
        }

        Ok(())
    }
}
