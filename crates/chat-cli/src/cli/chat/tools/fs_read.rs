use std::collections::VecDeque;
use std::fs::Metadata;
use std::io::Write;

use crossterm::queue;
use crossterm::style::{
    self,
    Color,
};
use eyre::{
    Result,
    bail,
};
use globset::GlobSetBuilder;
use serde::{
    Deserialize,
    Serialize,
};
use syntect::util::LinesWithEndings;
use tracing::{
    debug,
    error,
    warn,
};

use super::{
    InvokeOutput,
    MAX_TOOL_RESPONSE_SIZE,
    OutputKind,
    format_path,
    sanitize_path_tool_arg,
};
use crate::cli::agent::{
    Agent,
    PermissionEvalResult,
};
use crate::cli::chat::tools::display_purpose;
use crate::cli::chat::util::images::{
    handle_images_from_paths,
    is_supported_image_type,
    pre_process,
};
use crate::cli::chat::{
    CONTINUATION_LINE,
    sanitize_unicode_tags,
};
use crate::os::Os;
use crate::util::directories;
use crate::util::pattern_matching::matches_any_pattern;

#[derive(Debug, Clone, Deserialize)]
pub struct FsRead {
    // For batch operations
    pub operations: Vec<FsReadOperation>,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "mode")]
pub enum FsReadOperation {
    Line(FsLine),
    Directory(FsDirectory),
    Search(FsSearch),
    Image(FsImage),
}

impl FsRead {
    pub async fn validate(&mut self, os: &Os) -> Result<()> {
        if self.operations.is_empty() {
            bail!("At least one operation must be provided");
        }
        for op in &mut self.operations {
            op.validate(os).await?;
        }
        Ok(())
    }

    pub async fn queue_description(&self, os: &Os, updates: &mut impl Write) -> Result<()> {
        if self.operations.len() == 1 {
            // Single operation - display without batch prefix
            self.operations[0].queue_description(os, updates).await
        } else {
            // Multiple operations - display as batch
            queue!(
                updates,
                style::Print("Batch fs_read operation with "),
                style::SetForegroundColor(Color::Green),
                style::Print(self.operations.len()),
                style::ResetColor,
                style::Print(" operations:\n")
            )?;

            // Display purpose if available for batch operations
            let _ = display_purpose(self.summary.as_ref(), updates);

            for (i, op) in self.operations.iter().enumerate() {
                queue!(updates, style::Print(format!("\n↱ Operation {}: ", i + 1)))?;
                op.queue_description(os, updates).await?;
            }
            Ok(())
        }
    }

    pub fn eval_perm(&self, os: &Os, agent: &Agent) -> PermissionEvalResult {
        #[derive(Debug, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Settings {
            #[serde(default)]
            allowed_paths: Vec<String>,
            #[serde(default)]
            denied_paths: Vec<String>,
            #[serde(default = "default_allow_read_only")]
            allow_read_only: bool,
        }

        fn default_allow_read_only() -> bool {
            true
        }

        let is_in_allowlist = matches_any_pattern(&agent.allowed_tools, "fs_read");
        match agent.tools_settings.get("fs_read") {
            Some(settings) => {
                let Settings {
                    allowed_paths,
                    denied_paths,
                    allow_read_only,
                } = match serde_json::from_value::<Settings>(settings.clone()) {
                    Ok(settings) => settings,
                    Err(e) => {
                        error!("Failed to deserialize tool settings for fs_read: {:?}", e);
                        return PermissionEvalResult::Ask;
                    },
                };
                let allow_set = {
                    let mut builder = GlobSetBuilder::new();
                    for path in &allowed_paths {
                        let Ok(path) = directories::canonicalizes_path(os, path) else {
                            continue;
                        };
                        if let Err(e) = directories::add_gitignore_globs(&mut builder, path.as_str()) {
                            warn!("Failed to create glob from path given: {path}: {e}. Ignoring.");
                        }
                    }
                    builder.build()
                };

                let mut sanitized_deny_list = Vec::<&String>::new();
                let deny_set = {
                    let mut builder = GlobSetBuilder::new();
                    for path in &denied_paths {
                        let Ok(processed_path) = directories::canonicalizes_path(os, path) else {
                            continue;
                        };
                        match directories::add_gitignore_globs(&mut builder, processed_path.as_str()) {
                            Ok(_) => {
                                // Note that we need to push twice here because for each rule we
                                // are creating two globs (one for file and one for directory)
                                sanitized_deny_list.push(path);
                                sanitized_deny_list.push(path);
                            },
                            Err(e) => warn!("Failed to create glob from path given: {path}: {e}. Ignoring."),
                        }
                    }
                    builder.build()
                };

                match (allow_set, deny_set) {
                    (Ok(allow_set), Ok(deny_set)) => {
                        let mut deny_list = Vec::<PermissionEvalResult>::new();
                        let mut ask = false;

                        for op in &self.operations {
                            match op {
                                FsReadOperation::Line(FsLine { path, .. })
                                | FsReadOperation::Directory(FsDirectory { path, .. })
                                | FsReadOperation::Search(FsSearch { path, .. }) => {
                                    let Ok(path) = directories::canonicalizes_path(os, path) else {
                                        ask = true;
                                        continue;
                                    };
                                    let denied_match_set = deny_set.matches(path.as_ref() as &str);
                                    if !denied_match_set.is_empty() {
                                        let deny_res = PermissionEvalResult::Deny({
                                            denied_match_set
                                                .iter()
                                                .filter_map(|i| sanitized_deny_list.get(*i).map(|s| (*s).clone()))
                                                .collect::<Vec<_>>()
                                        });
                                        deny_list.push(deny_res);
                                        continue;
                                    }

                                    // We only want to ask if we are not allowing read only
                                    // operation
                                    if !is_in_allowlist
                                        && !allow_read_only
                                        && !allow_set.is_match(path.as_ref() as &str)
                                    {
                                        ask = true;
                                    }
                                },
                                FsReadOperation::Image(fs_image) => {
                                    let paths = &fs_image.image_paths;
                                    let denied_match_set = paths
                                        .iter()
                                        .flat_map(|path| {
                                            let Ok(path) = directories::canonicalizes_path(os, path) else {
                                                return vec![];
                                            };
                                            deny_set.matches(path.as_ref() as &str)
                                        })
                                        .collect::<Vec<_>>();
                                    if !denied_match_set.is_empty() {
                                        let deny_res = PermissionEvalResult::Deny({
                                            denied_match_set
                                                .iter()
                                                .filter_map(|i| sanitized_deny_list.get(*i).map(|s| (*s).clone()))
                                                .collect::<Vec<_>>()
                                        });
                                        deny_list.push(deny_res);
                                        continue;
                                    }

                                    // We only want to ask if we are not allowing read only
                                    // operation
                                    if !is_in_allowlist
                                        && !allow_read_only
                                        && !paths.iter().any(|path| allow_set.is_match(path))
                                    {
                                        ask = true;
                                    }
                                },
                            }
                        }

                        if !deny_list.is_empty() {
                            PermissionEvalResult::Deny({
                                deny_list.into_iter().fold(Vec::<String>::new(), |mut acc, res| {
                                    if let PermissionEvalResult::Deny(mut rules) = res {
                                        acc.append(&mut rules);
                                    }
                                    acc
                                })
                            })
                        } else if ask {
                            PermissionEvalResult::Ask
                        } else {
                            PermissionEvalResult::Allow
                        }
                    },
                    (allow_res, deny_res) => {
                        if let Err(e) = allow_res {
                            warn!("fs_read failed to build allow set: {:?}", e);
                        }
                        if let Err(e) = deny_res {
                            warn!("fs_read failed to build deny set: {:?}", e);
                        }
                        warn!("One or more detailed args failed to parse, falling back to ask");
                        PermissionEvalResult::Ask
                    },
                }
            },
            None if is_in_allowlist => PermissionEvalResult::Allow,
            _ => PermissionEvalResult::Ask,
        }
    }

    pub async fn invoke(&self, os: &Os, updates: &mut impl Write) -> Result<InvokeOutput> {
        if self.operations.len() == 1 {
            // Single operation - return result directly
            self.operations[0].invoke(os, updates).await
        } else {
            // Multiple operations - combine results
            let mut combined_results = Vec::new();
            let mut all_images = Vec::new();
            let mut has_non_image_ops = false;
            let mut success_ops = 0usize;
            let mut failed_ops = 0usize;

            for (i, op) in self.operations.iter().enumerate() {
                match op.invoke(os, updates).await {
                    Ok(result) => {
                        success_ops += 1;

                        match &result.output {
                            OutputKind::Text(text) => {
                                combined_results.push(format!("=== Operation {} Result (Text) ===\n{}", i + 1, text));
                                has_non_image_ops = true;
                            },
                            OutputKind::Json(json) => {
                                combined_results.push(format!(
                                    "=== Operation {} Result (Json) ===\n{}",
                                    i + 1,
                                    serde_json::to_string_pretty(json)?
                                ));
                                has_non_image_ops = true;
                            },
                            OutputKind::Images(images) => {
                                all_images.extend(images.clone());
                                combined_results.push(format!(
                                    "=== Operation {} Result (Images) ===\n[{} images processed]",
                                    i + 1,
                                    images.len()
                                ));
                            },
                            // This branch won't be reached because single operation execution never returns a Mixed
                            // result
                            OutputKind::Mixed { text: _, images: _ } => {},
                        }
                    },

                    Err(err) => {
                        failed_ops += 1;
                        combined_results.push(format!("=== Operation {} Error ===\n{}", i + 1, err));
                    },
                }
            }

            queue!(
                updates,
                style::Print("\n"),
                style::Print(CONTINUATION_LINE),
                style::Print("\n")
            )?;
            super::queue_function_result(
                &format!(
                    "Summary: {} operations processed, {} successful, {} failed",
                    self.operations.len(),
                    success_ops,
                    failed_ops
                ),
                updates,
                false,
                true,
            )?;

            let combined_text = combined_results.join("\n\n");

            if !all_images.is_empty() && has_non_image_ops {
                Ok(InvokeOutput {
                    output: OutputKind::Mixed {
                        text: combined_text,
                        images: all_images,
                    },
                })
            } else if !all_images.is_empty() {
                Ok(InvokeOutput {
                    output: OutputKind::Images(all_images),
                })
            } else {
                Ok(InvokeOutput {
                    output: OutputKind::Text(combined_text),
                })
            }
        }
    }
}

impl FsReadOperation {
    pub async fn validate(&mut self, os: &Os) -> Result<()> {
        match self {
            FsReadOperation::Line(fs_line) => fs_line.validate(os).await,
            FsReadOperation::Directory(fs_directory) => fs_directory.validate(os).await,
            FsReadOperation::Search(fs_search) => fs_search.validate(os).await,
            FsReadOperation::Image(fs_image) => fs_image.validate(os).await,
        }
    }

    pub async fn queue_description(&self, os: &Os, updates: &mut impl Write) -> Result<()> {
        match self {
            FsReadOperation::Line(fs_line) => fs_line.queue_description(os, updates).await,
            FsReadOperation::Directory(fs_directory) => fs_directory.queue_description(updates),
            FsReadOperation::Search(fs_search) => fs_search.queue_description(updates),
            FsReadOperation::Image(fs_image) => fs_image.queue_description(updates),
        }
    }

    pub async fn invoke(&self, os: &Os, updates: &mut impl Write) -> Result<InvokeOutput> {
        match self {
            FsReadOperation::Line(fs_line) => fs_line.invoke(os, updates).await,
            FsReadOperation::Directory(fs_directory) => fs_directory.invoke(os, updates).await,
            FsReadOperation::Search(fs_search) => fs_search.invoke(os, updates).await,
            FsReadOperation::Image(fs_image) => fs_image.invoke(updates).await,
        }
    }
}

/// Read images from given paths.
#[derive(Debug, Clone, Deserialize)]
pub struct FsImage {
    pub image_paths: Vec<String>,
}

impl FsImage {
    pub async fn validate(&mut self, os: &Os) -> Result<()> {
        for path in &self.image_paths {
            let path = sanitize_path_tool_arg(os, path);
            if let Some(path) = path.to_str() {
                let processed_path = pre_process(path);
                if !is_supported_image_type(&processed_path) {
                    bail!("'{}' is not a supported image type", &processed_path);
                }
                let is_file = os.fs.symlink_metadata(&processed_path).await?.is_file();
                if !is_file {
                    bail!("'{}' is not a file", &processed_path);
                }
            } else {
                bail!("Unable to parse path");
            }
        }
        Ok(())
    }

    pub async fn invoke(&self, updates: &mut impl Write) -> Result<InvokeOutput> {
        let pre_processed_paths: Vec<String> = self.image_paths.iter().map(|path| pre_process(path)).collect();
        let valid_images = handle_images_from_paths(updates, &pre_processed_paths);
        super::queue_function_result("Successfully read image", updates, false, false)?;
        Ok(InvokeOutput {
            output: OutputKind::Images(valid_images),
        })
    }

    pub fn queue_description(&self, updates: &mut impl Write) -> Result<()> {
        queue!(
            updates,
            style::Print("Reading images: "),
            style::SetForegroundColor(Color::Green),
            style::Print(&self.image_paths.join("\n")),
            style::Print("\n"),
            style::ResetColor,
        )?;
        Ok(())
    }
}

/// Read lines from a file.
#[derive(Debug, Clone, Deserialize)]
pub struct FsLine {
    pub path: String,
    pub start_line: Option<i32>,
    pub end_line: Option<i32>,
}

impl FsLine {
    const DEFAULT_END_LINE: i32 = -1;
    const DEFAULT_START_LINE: i32 = 1;

    pub async fn validate(&mut self, os: &Os) -> Result<()> {
        let path = sanitize_path_tool_arg(os, &self.path);
        if !path.exists() {
            bail!("'{}' does not exist", self.path);
        }
        let is_file = os.fs.symlink_metadata(&path).await?.is_file();
        if !is_file {
            bail!("'{}' is not a file", self.path);
        }
        Ok(())
    }

    pub async fn queue_description(&self, os: &Os, updates: &mut impl Write) -> Result<()> {
        let path = sanitize_path_tool_arg(os, &self.path);
        let file_bytes = os.fs.read(&path).await?;
        let file_content = String::from_utf8_lossy(&file_bytes);
        let line_count = file_content.lines().count();
        queue!(
            updates,
            style::Print("Reading file: "),
            style::SetForegroundColor(Color::Green),
            style::Print(&self.path),
            style::ResetColor,
            style::Print(", "),
        )?;

        let start = convert_negative_index(line_count, self.start_line()) + 1;
        let end = convert_negative_index(line_count, self.end_line()) + 1;
        match (start, end) {
            _ if start == 1 && end == line_count => Ok(queue!(updates, style::Print("all lines".to_string()))?),
            _ if end == line_count => Ok(queue!(
                updates,
                style::Print("from line "),
                style::SetForegroundColor(Color::Green),
                style::Print(start),
                style::ResetColor,
                style::Print(" to end of file"),
            )?),
            _ => Ok(queue!(
                updates,
                style::Print("from line "),
                style::SetForegroundColor(Color::Green),
                style::Print(start),
                style::ResetColor,
                style::Print(" to "),
                style::SetForegroundColor(Color::Green),
                style::Print(end),
                style::ResetColor,
            )?),
        }
    }

    pub async fn invoke(&self, os: &Os, updates: &mut impl Write) -> Result<InvokeOutput> {
        let path = sanitize_path_tool_arg(os, &self.path);
        debug!(?path, "Reading");
        let file_bytes = os.fs.read(&path).await?;
        let file_content = String::from_utf8_lossy(&file_bytes);
        let file_content = sanitize_unicode_tags(&file_content);
        let line_count = file_content.lines().count();
        let (start, end) = (
            convert_negative_index(line_count, self.start_line()),
            convert_negative_index(line_count, self.end_line()),
        );

        // safety check to ensure end is always greater than start
        let end = end.max(start);

        if start >= line_count {
            bail!(
                "starting index: {} is outside of the allowed range: ({}, {})",
                self.start_line(),
                -(line_count as i64),
                line_count
            );
        }

        // The range should be inclusive on both ends.
        let file_contents = file_content
            .lines()
            .skip(start)
            .take(end - start + 1)
            .collect::<Vec<_>>()
            .join("\n");

        let byte_count = file_contents.len();
        if byte_count > MAX_TOOL_RESPONSE_SIZE {
            bail!(
                "This tool only supports reading {MAX_TOOL_RESPONSE_SIZE} bytes at a
time. You tried to read {byte_count} bytes. Try executing with fewer lines specified."
            );
        }

        super::queue_function_result(
            &format!(
                "Successfully read {} bytes from {}",
                file_contents.len(),
                &path.display()
            ),
            updates,
            false,
            false,
        )?;

        Ok(InvokeOutput {
            output: OutputKind::Text(file_contents),
        })
    }

    fn start_line(&self) -> i32 {
        self.start_line.unwrap_or(Self::DEFAULT_START_LINE)
    }

    fn end_line(&self) -> i32 {
        self.end_line.unwrap_or(Self::DEFAULT_END_LINE)
    }
}

/// Search in a file.
#[derive(Debug, Clone, Deserialize)]
pub struct FsSearch {
    pub path: String,
    pub pattern: String,
    pub context_lines: Option<usize>,
}

impl FsSearch {
    const CONTEXT_LINE_PREFIX: &str = "  ";
    const DEFAULT_CONTEXT_LINES: usize = 2;
    const MATCHING_LINE_PREFIX: &str = "→ ";

    pub async fn validate(&mut self, os: &Os) -> Result<()> {
        let path = sanitize_path_tool_arg(os, &self.path);
        let relative_path = format_path(os.env.current_dir()?, &path);
        if !path.exists() {
            bail!("File not found: {}", relative_path);
        }
        if !os.fs.symlink_metadata(path).await?.is_file() {
            bail!("Path is not a file: {}", relative_path);
        }
        if self.pattern.is_empty() {
            bail!("Search pattern cannot be empty");
        }
        Ok(())
    }

    pub fn queue_description(&self, updates: &mut impl Write) -> Result<()> {
        queue!(
            updates,
            style::Print("Searching: "),
            style::SetForegroundColor(Color::Green),
            style::Print(&self.path),
            style::ResetColor,
            style::Print(" for pattern: "),
            style::SetForegroundColor(Color::Green),
            style::Print(&self.pattern.to_lowercase()),
            style::ResetColor,
        )?;
        Ok(())
    }

    pub async fn invoke(&self, os: &Os, updates: &mut impl Write) -> Result<InvokeOutput> {
        let file_path = sanitize_path_tool_arg(os, &self.path);
        let pattern = &self.pattern;

        let file_bytes = os.fs.read(&file_path).await?;
        let file_content = String::from_utf8_lossy(&file_bytes);
        let file_content = sanitize_unicode_tags(&file_content);
        let lines: Vec<&str> = LinesWithEndings::from(&file_content).collect();

        let mut results = Vec::new();
        let mut total_matches = 0;

        // Case insensitive search
        let pattern_lower = pattern.to_lowercase();
        for (line_num, line) in lines.iter().enumerate() {
            if line.to_lowercase().contains(&pattern_lower) {
                total_matches += 1;
                let start = line_num.saturating_sub(self.context_lines());
                let end = lines.len().min(line_num + self.context_lines() + 1);
                let mut context_text = Vec::new();
                (start..end).for_each(|i| {
                    let prefix = if i == line_num {
                        Self::MATCHING_LINE_PREFIX
                    } else {
                        Self::CONTEXT_LINE_PREFIX
                    };
                    let line_text = lines[i].to_string();
                    context_text.push(format!("{}{}: {}", prefix, i + 1, line_text));
                });
                let match_text = context_text.join("");
                results.push(SearchMatch {
                    line_number: line_num + 1,
                    context: match_text,
                });
            }
        }

        super::queue_function_result(
            &format!(
                "Found {} matches for pattern '{}' in {}",
                total_matches,
                pattern,
                &file_path.display()
            ),
            updates,
            false,
            false,
        )?;

        Ok(InvokeOutput {
            output: OutputKind::Text(serde_json::to_string(&results)?),
        })
    }

    fn context_lines(&self) -> usize {
        self.context_lines.unwrap_or(Self::DEFAULT_CONTEXT_LINES)
    }
}

/// List directory contents.
#[derive(Debug, Clone, Deserialize)]
pub struct FsDirectory {
    pub path: String,
    pub depth: Option<usize>,
}

impl FsDirectory {
    const DEFAULT_DEPTH: usize = 0;

    pub async fn validate(&mut self, os: &Os) -> Result<()> {
        let path = sanitize_path_tool_arg(os, &self.path);
        let relative_path = format_path(os.env.current_dir()?, &path);
        if !path.exists() {
            bail!("Directory not found: {}", relative_path);
        }
        if !os.fs.symlink_metadata(path).await?.is_dir() {
            bail!("Path is not a directory: {}", relative_path);
        }
        Ok(())
    }

    pub fn queue_description(&self, updates: &mut impl Write) -> Result<()> {
        queue!(
            updates,
            style::Print("Reading directory: "),
            style::SetForegroundColor(Color::Green),
            style::Print(&self.path),
            style::ResetColor,
            style::Print(" "),
        )?;
        let depth = self.depth.unwrap_or_default();
        Ok(queue!(
            updates,
            style::Print(format!("with maximum depth of {}", depth))
        )?)
    }

    pub async fn invoke(&self, os: &Os, updates: &mut impl Write) -> Result<InvokeOutput> {
        let path = sanitize_path_tool_arg(os, &self.path);
        let max_depth = self.depth();
        debug!(?path, max_depth, "Reading directory at path with depth");
        let mut result = Vec::new();
        let mut dir_queue = VecDeque::new();
        dir_queue.push_back((path.clone(), 0));
        while let Some((path, depth)) = dir_queue.pop_front() {
            if depth > max_depth {
                break;
            }
            let mut read_dir = os.fs.read_dir(path).await?;

            #[cfg(windows)]
            while let Some(ent) = read_dir.next_entry().await? {
                let md = ent.metadata().await?;

                let modified_timestamp = md.modified()?.duration_since(std::time::UNIX_EPOCH)?.as_secs();
                let datetime = time::OffsetDateTime::from_unix_timestamp(modified_timestamp as i64).unwrap();
                let formatted_date = datetime
                    .format(time::macros::format_description!(
                        "[month repr:short] [day] [hour]:[minute]"
                    ))
                    .unwrap();

                result.push(format!(
                    "{} {} {} {}",
                    format_ftype(&md),
                    String::from_utf8_lossy(ent.file_name().as_encoded_bytes()),
                    formatted_date,
                    ent.path().to_string_lossy()
                ));

                if md.is_dir() && md.is_dir() {
                    dir_queue.push_back((ent.path(), depth + 1));
                }
            }

            #[cfg(unix)]
            while let Some(ent) = read_dir.next_entry().await? {
                use std::os::unix::fs::{
                    MetadataExt,
                    PermissionsExt,
                };

                let md = ent.metadata().await?;
                let formatted_mode = format_mode(md.permissions().mode()).into_iter().collect::<String>();

                let modified_timestamp = md.modified()?.duration_since(std::time::UNIX_EPOCH)?.as_secs();
                let datetime = time::OffsetDateTime::from_unix_timestamp(modified_timestamp as i64).unwrap();
                let formatted_date = datetime
                    .format(time::macros::format_description!(
                        "[month repr:short] [day] [hour]:[minute]"
                    ))
                    .unwrap();

                // Mostly copying "The Long Format" from `man ls`.
                // TODO: query user/group database to convert uid/gid to names?
                result.push(format!(
                    "{}{} {} {} {} {} {} {}",
                    format_ftype(&md),
                    formatted_mode,
                    md.nlink(),
                    md.uid(),
                    md.gid(),
                    md.size(),
                    formatted_date,
                    ent.path().to_string_lossy()
                ));
                if md.is_dir() {
                    dir_queue.push_back((ent.path(), depth + 1));
                }
            }
        }

        let file_count = result.len();
        let result = result.join("\n");
        let byte_count = result.len();
        if byte_count > MAX_TOOL_RESPONSE_SIZE {
            bail!(
                "This tool only supports reading up to {MAX_TOOL_RESPONSE_SIZE} bytes at a time. You tried to read {byte_count} bytes ({file_count} files). Try executing with fewer lines specified."
            );
        }

        super::queue_function_result(
            &format!(
                "Successfully read directory {} ({} entries)",
                &path.display(),
                file_count
            ),
            updates,
            false,
            false,
        )?;

        Ok(InvokeOutput {
            output: OutputKind::Text(result),
        })
    }

    fn depth(&self) -> usize {
        self.depth.unwrap_or(Self::DEFAULT_DEPTH)
    }
}

/// Converts negative 1-based indices to positive 0-based indices.
fn convert_negative_index(line_count: usize, i: i32) -> usize {
    if i <= 0 {
        (line_count as i32 + i).max(0) as usize
    } else {
        i as usize - 1
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SearchMatch {
    line_number: usize,
    context: String,
}

fn format_ftype(md: &Metadata) -> char {
    if md.is_symlink() {
        'l'
    } else if md.is_file() {
        '-'
    } else if md.is_dir() {
        'd'
    } else {
        warn!("unknown file metadata: {:?}", md);
        '-'
    }
}

/// Formats a permissions mode into the form used by `ls`, e.g. `0o644` to `rw-r--r--`
#[cfg(unix)]
fn format_mode(mode: u32) -> [char; 9] {
    let mut mode = mode & 0o777;
    let mut res = ['-'; 9];
    fn octal_to_chars(val: u32) -> [char; 3] {
        match val {
            1 => ['-', '-', 'x'],
            2 => ['-', 'w', '-'],
            3 => ['-', 'w', 'x'],
            4 => ['r', '-', '-'],
            5 => ['r', '-', 'x'],
            6 => ['r', 'w', '-'],
            7 => ['r', 'w', 'x'],
            _ => ['-', '-', '-'],
        }
    }
    for c in res.rchunks_exact_mut(3) {
        c.copy_from_slice(&octal_to_chars(mode & 0o7));
        mode /= 0o10;
    }
    res
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::cli::agent::ToolSettingTarget;
    use crate::cli::chat::util::test::{
        TEST_FILE_CONTENTS,
        TEST_FILE_PATH,
        setup_test_directory,
    };

    #[test]
    fn test_negative_index_conversion() {
        assert_eq!(convert_negative_index(5, -100), 0);
        assert_eq!(convert_negative_index(5, -1), 4);
    }

    #[test]
    fn test_fs_read_deser() {
        // Test single operations (wrapped in operations array)
        serde_json::from_value::<FsRead>(
            serde_json::json!({ "operations": [{ "path": "/test_file.txt", "mode": "Line" }] }),
        )
        .unwrap();
        serde_json::from_value::<FsRead>(
            serde_json::json!({ "operations": [{ "path": "/test_file.txt", "mode": "Line", "end_line": 5 }] }),
        )
        .unwrap();
        serde_json::from_value::<FsRead>(
            serde_json::json!({ "operations": [{ "path": "/test_file.txt", "mode": "Line", "start_line": -1 }]  }),
        )
        .unwrap();
        serde_json::from_value::<FsRead>(
            serde_json::json!({ "operations": [{ "path": "/test_file.txt", "mode": "Line", "start_line": None::<usize> }] }),
        )
        .unwrap();
        serde_json::from_value::<FsRead>(serde_json::json!({ "operations": [{ "path": "/", "mode": "Directory" }] }))
            .unwrap();
        serde_json::from_value::<FsRead>(
            serde_json::json!({ "operations": [{ "path": "/test_file.txt", "mode": "Directory", "depth": 2 }] }),
        )
        .unwrap();
        serde_json::from_value::<FsRead>(
            serde_json::json!({ "operations": [{ "path": "/test_file.txt", "mode": "Search", "pattern": "hello" }] }),
        )
        .unwrap();
        serde_json::from_value::<FsRead>(serde_json::json!({
            "operations": [{ "image_paths": ["/img1.png", "/img2.jpg"], "mode": "Image" }]
        }))
        .unwrap();

        // Test mixed batch operations
        serde_json::from_value::<FsRead>(serde_json::json!({
            "operations": [
                { "path": "/file.txt", "mode": "Line" },
                { "path": "/dir", "mode": "Directory", "depth": 1 },
                { "path": "/log.txt", "mode": "Search", "pattern": "warning" },
                { "image_paths": ["/photo.jpg"], "mode": "Image" }
            ],
            "purpose": "Comprehensive file analysis"
        }))
        .unwrap();
    }

    #[tokio::test]
    async fn test_fs_read_line_invoke() {
        let os = setup_test_directory().await;
        let lines = TEST_FILE_CONTENTS.lines().collect::<Vec<_>>();
        let mut stdout = std::io::stdout();

        macro_rules! assert_lines {
            ($start_line:expr, $end_line:expr, $expected:expr) => {
                let v = serde_json::json!({
                    "operations": [{
                    "path": TEST_FILE_PATH,
                    "mode": "Line",
                    "start_line": $start_line,
                    "end_line": $end_line,}]
                });
                let output = serde_json::from_value::<FsRead>(v)
                    .unwrap()
                    .invoke(&os, &mut stdout)
                    .await
                    .unwrap();

                if let OutputKind::Text(text) = output.output {
                    assert_eq!(text, $expected.join("\n"), "actual(left) does not equal
                                expected(right) for (start_line, end_line): ({:?}, {:?})", $start_line, $end_line);
                } else {
                    panic!("expected text output");
                }
            }
        }
        assert_lines!(None::<i32>, None::<i32>, lines[..]);
        assert_lines!(1, 2, lines[..=1]);
        assert_lines!(1, -1, lines[..]);
        assert_lines!(2, 1, lines[1..=1]);
        assert_lines!(-2, -1, lines[2..]);
        assert_lines!(-2, None::<i32>, lines[2..]);
        assert_lines!(2, None::<i32>, lines[1..]);
    }

    #[tokio::test]
    async fn test_fs_read_line_past_eof() {
        let os = setup_test_directory().await;
        let mut stdout = std::io::stdout();
        let v = serde_json::json!({
            "operations": [{
            "path": TEST_FILE_PATH,
            "mode": "Line",
            "start_line": 100,
            "end_line": None::<i32>,}]});
        assert!(
            serde_json::from_value::<FsRead>(v)
                .unwrap()
                .invoke(&os, &mut stdout)
                .await
                .is_err()
        );
    }

    #[test]
    #[cfg(unix)]
    fn test_format_mode() {
        macro_rules! assert_mode {
            ($actual:expr, $expected:expr) => {
                assert_eq!(format_mode($actual).iter().collect::<String>(), $expected);
            };
        }
        assert_mode!(0o000, "---------");
        assert_mode!(0o700, "rwx------");
        assert_mode!(0o744, "rwxr--r--");
        assert_mode!(0o641, "rw-r----x");
    }

    #[tokio::test]
    async fn test_fs_read_directory_invoke() {
        let os = setup_test_directory().await;
        let mut stdout = std::io::stdout();

        // Testing without depth
        let v = serde_json::json!({
            "operations": [{
            "mode": "Directory",
            "path": "/",
        }]});
        let output = serde_json::from_value::<FsRead>(v)
            .unwrap()
            .invoke(&os, &mut stdout)
            .await
            .unwrap();

        if let OutputKind::Text(text) = output.output {
            assert_eq!(text.lines().collect::<Vec<_>>().len(), 4);
        } else {
            panic!("expected text output");
        }

        // Testing with depth level 1
        let v = serde_json::json!({
            "operations": [{
            "mode": "Directory",
            "path": "/",
            "depth": 1,}]
        });
        let output = serde_json::from_value::<FsRead>(v)
            .unwrap()
            .invoke(&os, &mut stdout)
            .await
            .unwrap();

        if let OutputKind::Text(text) = output.output {
            let lines = text.lines().collect::<Vec<_>>();
            assert_eq!(lines.len(), 7);
            assert!(
                !lines.iter().any(|l| l.contains("cccc1")),
                "directory at depth level 2 should not be included in output"
            );
        } else {
            panic!("expected text output");
        }
    }

    #[tokio::test]
    async fn test_fs_read_search_invoke() {
        let os = setup_test_directory().await;
        let mut stdout = std::io::stdout();

        macro_rules! invoke_search {
            ($value:tt) => {{
                let v = serde_json::json!($value);
                let output = serde_json::from_value::<FsRead>(v)
                    .unwrap()
                    .invoke(&os, &mut stdout)
                    .await
                    .unwrap();

                if let OutputKind::Text(value) = output.output {
                    serde_json::from_str::<Vec<SearchMatch>>(&value).unwrap()
                } else {
                    panic!("expected Text output")
                }
            }};
        }

        let matches = invoke_search!({
            "operations": [{
            "mode": "Search",
            "path": TEST_FILE_PATH,
            "pattern": "hello",}]
        });
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].line_number, 1);
        assert_eq!(
            matches[0].context,
            format!(
                "{}1: 1: Hello world!\n{}2: 2: This is line 2\n{}3: 3: asdf\n",
                FsSearch::MATCHING_LINE_PREFIX,
                FsSearch::CONTEXT_LINE_PREFIX,
                FsSearch::CONTEXT_LINE_PREFIX
            )
        );
    }

    #[tokio::test]
    async fn test_fs_read_non_utf8_binary_file() {
        let os = Os::new().await.unwrap();
        let mut stdout = std::io::stdout();

        let binary_data = vec![0xff, 0xfe, 0xfd, 0xfc, 0xfb, 0xfa, 0xf9, 0xf8];
        let binary_file_path = "/binary_test.dat";
        os.fs.write(binary_file_path, &binary_data).await.unwrap();

        let v = serde_json::json!({
            "operations": [{
            "path": binary_file_path,
            "mode": "Line"}]
        });
        let output = serde_json::from_value::<FsRead>(v)
            .unwrap()
            .invoke(&os, &mut stdout)
            .await
            .unwrap();

        if let OutputKind::Text(text) = output.output {
            assert!(text.contains('�'), "Binary data should contain replacement characters");
            assert_eq!(text.chars().count(), 8, "Should have 8 replacement characters");
            assert!(
                text.chars().all(|c| c == '�'),
                "All characters should be replacement characters"
            );
        } else {
            panic!("expected text output");
        }
    }

    #[tokio::test]
    async fn test_fs_read_latin1_encoded_file() {
        let os = Os::new().await.unwrap();
        let mut stdout = std::io::stdout();

        let latin1_data = vec![99, 97, 102, 233]; // "café" in Latin-1
        let latin1_file_path = "/latin1_test.txt";
        os.fs.write(latin1_file_path, &latin1_data).await.unwrap();

        let v = serde_json::json!({
            "operations": [{
            "path": latin1_file_path,
            "mode": "Line"}]
        });
        let output = serde_json::from_value::<FsRead>(v)
            .unwrap()
            .invoke(&os, &mut stdout)
            .await
            .unwrap();

        if let OutputKind::Text(text) = output.output {
            // Latin-1 byte 233 (é) is invalid UTF-8, so it becomes a replacement character
            assert!(text.starts_with("caf"), "Should start with 'caf'");
            assert!(
                text.contains('�'),
                "Should contain replacement character for invalid UTF-8"
            );
        } else {
            panic!("expected text output");
        }
    }

    #[tokio::test]
    async fn test_fs_search_non_utf8_file() {
        let os = Os::new().await.unwrap();
        let mut stdout = std::io::stdout();

        let mut mixed_data = Vec::new();
        mixed_data.extend_from_slice(b"Hello world\n");
        mixed_data.extend_from_slice(&[0xff, 0xfe]); // Invalid UTF-8 bytes
        mixed_data.extend_from_slice(b"\nGoodbye world\n");

        let mixed_file_path = "/mixed_encoding_test.txt";
        os.fs.write(mixed_file_path, &mixed_data).await.unwrap();

        let v = serde_json::json!({
            "operations": [{
            "mode": "Search",
            "path": mixed_file_path,
            "pattern": "hello"}]
        });
        let output = serde_json::from_value::<FsRead>(v)
            .unwrap()
            .invoke(&os, &mut stdout)
            .await
            .unwrap();

        if let OutputKind::Text(value) = output.output {
            let matches: Vec<SearchMatch> = serde_json::from_str(&value).unwrap();
            assert_eq!(matches.len(), 1, "Should find one match for 'hello'");
            assert_eq!(matches[0].line_number, 1, "Match should be on line 1");
            assert!(
                matches[0].context.contains("Hello world"),
                "Should contain the matched line"
            );
        } else {
            panic!("expected Text output");
        }

        let v = serde_json::json!({
            "operations": [{
            "mode": "Search",
            "path": mixed_file_path,
            "pattern": "goodbye"}]
        });
        let output = serde_json::from_value::<FsRead>(v)
            .unwrap()
            .invoke(&os, &mut stdout)
            .await
            .unwrap();

        if let OutputKind::Text(value) = output.output {
            let matches: Vec<SearchMatch> = serde_json::from_str(&value).unwrap();
            assert_eq!(matches.len(), 1, "Should find one match for 'goodbye'");
            assert!(
                matches[0].context.contains("Goodbye world"),
                "Should contain the matched line"
            );
        } else {
            panic!("expected Text output");
        }
    }

    #[tokio::test]
    async fn test_fs_read_windows1252_encoded_file() {
        let os = Os::new().await.unwrap();
        let mut stdout = std::io::stdout();

        let mut windows1252_data = Vec::new();
        windows1252_data.extend_from_slice(b"Text with ");
        windows1252_data.push(0x93); // Left double quotation mark in Windows-1252
        windows1252_data.extend_from_slice(b"smart quotes");
        windows1252_data.push(0x94); // Right double quotation mark in Windows-1252

        let windows1252_file_path = "/windows1252_test.txt";
        os.fs.write(windows1252_file_path, &windows1252_data).await.unwrap();

        let v = serde_json::json!({
            "operations": [{
            "path": windows1252_file_path,
            "mode": "Line"}]
        });
        let output = serde_json::from_value::<FsRead>(v)
            .unwrap()
            .invoke(&os, &mut stdout)
            .await
            .unwrap();

        if let OutputKind::Text(text) = output.output {
            assert!(text.contains("Text with"), "Should contain readable text");
            assert!(text.contains("smart quotes"), "Should contain readable text");
            assert!(
                text.contains('�'),
                "Should contain replacement characters for invalid UTF-8"
            );
        } else {
            panic!("expected text output");
        }
    }

    #[tokio::test]
    async fn test_fs_search_pattern_with_replacement_chars() {
        let os = Os::new().await.unwrap();
        let mut stdout = std::io::stdout();

        let mut data_with_invalid_utf8 = Vec::new();
        data_with_invalid_utf8.extend_from_slice(b"Line 1: caf");
        data_with_invalid_utf8.push(0xe9); // Invalid UTF-8 byte (Latin-1 é)
        data_with_invalid_utf8.extend_from_slice(b"\nLine 2: hello world\n");

        let invalid_utf8_file_path = "/invalid_utf8_search_test.txt";
        os.fs
            .write(invalid_utf8_file_path, &data_with_invalid_utf8)
            .await
            .unwrap();

        let v = serde_json::json!({
            "operations": [{
            "mode": "Search",
            "path": invalid_utf8_file_path,
            "pattern": "caf"}]
        });
        let output = serde_json::from_value::<FsRead>(v)
            .unwrap()
            .invoke(&os, &mut stdout)
            .await
            .unwrap();

        if let OutputKind::Text(value) = output.output {
            let matches: Vec<SearchMatch> = serde_json::from_str(&value).unwrap();
            assert_eq!(matches.len(), 1, "Should find one match for 'caf'");
            assert_eq!(matches[0].line_number, 1, "Match should be on line 1");
            assert!(matches[0].context.contains("caf"), "Should contain 'caf'");
        } else {
            panic!("expected Text output");
        }
    }

    #[tokio::test]
    async fn test_fs_read_empty_file_with_invalid_utf8() {
        let os = Os::new().await.unwrap();
        let mut stdout = std::io::stdout();

        let invalid_only_data = vec![0xff, 0xfe, 0xfd];
        let invalid_only_file_path = "/invalid_only_test.txt";
        os.fs.write(invalid_only_file_path, &invalid_only_data).await.unwrap();

        let v = serde_json::json!({
            "operations": [{
            "path": invalid_only_file_path,
            "mode": "Line"}]
        });
        let output = serde_json::from_value::<FsRead>(v)
            .unwrap()
            .invoke(&os, &mut stdout)
            .await
            .unwrap();

        if let OutputKind::Text(text) = output.output {
            assert_eq!(text.chars().count(), 3, "Should have 3 replacement characters");
            assert!(text.chars().all(|c| c == '�'), "Should be all replacement characters");
        } else {
            panic!("expected text output");
        }

        let v = serde_json::json!({
            "operations": [{
            "mode": "Search",
            "path": invalid_only_file_path,
            "pattern": "test"}]
        });
        let output = serde_json::from_value::<FsRead>(v)
            .unwrap()
            .invoke(&os, &mut stdout)
            .await
            .unwrap();

        if let OutputKind::Text(value) = output.output {
            let matches: Vec<SearchMatch> = serde_json::from_str(&value).unwrap();
            assert_eq!(
                matches.len(),
                0,
                "Should find no matches in file with only invalid UTF-8"
            );
        } else {
            panic!("expected Text output");
        }
    }

    #[tokio::test]
    async fn test_fs_read_batch_mixed_operations() {
        let os = setup_test_directory().await;
        let mut stdout = Vec::new();

        let v = serde_json::json!({
            "operations": [
                { "path": TEST_FILE_PATH, "mode": "Line", "start_line": 1, "end_line": 2 },
                { "path": "/", "mode": "Directory" },
                { "path": TEST_FILE_PATH, "mode": "Search", "pattern": "hello" }
            ],
            "purpose": "Test mixed text operations"
        });

        let output = serde_json::from_value::<FsRead>(v)
            .unwrap()
            .invoke(&os, &mut stdout)
            .await
            .unwrap();
        // All text operations should return combined text
        if let OutputKind::Text(text) = output.output {
            // Check all operations are included
            assert!(text.contains("=== Operation 1 Result (Text) ==="));
            assert!(text.contains("=== Operation 2 Result (Text) ==="));
            assert!(text.contains("=== Operation 3 Result (Text) ==="));

            // Check operation 1 (Line mode)
            assert!(text.contains("Hello world!"));
            assert!(text.contains("This is line 2"));

            // Check operation 2 (Directory mode)
            assert!(text.contains("test_file.txt"));

            // Check operation 3 (Search mode)
            assert!(text.contains("\"line_number\":1"));
        } else {
            panic!("expected text output for batch operations");
        }
    }

    #[tokio::test]
    async fn test_fs_read_empty_operations() {
        let os = Os::new().await.unwrap();

        // Test empty operations array
        let v = serde_json::json!({
            "operations": []
        });

        let mut fs_read = serde_json::from_value::<FsRead>(v).unwrap();
        let result = fs_read.validate(&os).await;

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("At least one operation must be provided")
        );
    }

    #[tokio::test]
    async fn test_eval_perm() {
        const DENIED_PATH_OR_FILE: &str = "/some/denied/path";
        const DENIED_PATH_OR_FILE_GLOB: &str = "/denied/glob/**/path";

        let mut agent = Agent {
            name: "test_agent".to_string(),
            tools_settings: {
                let mut map = HashMap::<ToolSettingTarget, serde_json::Value>::new();
                map.insert(
                    ToolSettingTarget("fs_read".to_string()),
                    serde_json::json!({
                        "deniedPaths": [DENIED_PATH_OR_FILE, DENIED_PATH_OR_FILE_GLOB]
                    }),
                );
                map
            },
            ..Default::default()
        };

        let os = Os::new().await.unwrap();

        let tool_one = serde_json::from_value::<FsRead>(serde_json::json!({
            "operations": [
                { "path": DENIED_PATH_OR_FILE, "mode": "Line", "start_line": 1, "end_line": 2 },
                { "path": format!("{DENIED_PATH_OR_FILE}/child"), "mode": "Line", "start_line": 1, "end_line": 2 },
                { "path": "/denied/glob/middle_one/middle_two/path", "mode": "Line", "start_line": 1, "end_line": 2 },
                { "path": "/denied/glob/middle_one/middle_two/path/child", "mode": "Line", "start_line": 1, "end_line": 2 },
            ],
        }))
        .unwrap();

        let res = tool_one.eval_perm(&os, &agent);
        assert!(matches!(
            res,
            PermissionEvalResult::Deny(ref deny_list)
                if deny_list.iter().filter(|p| *p == DENIED_PATH_OR_FILE_GLOB).collect::<Vec<_>>().len() == 2
                && deny_list.iter().filter(|p| *p == DENIED_PATH_OR_FILE).collect::<Vec<_>>().len() == 2
        ));

        agent.allowed_tools.insert("fs_read".to_string());

        // Denied set should remain denied
        let res = tool_one.eval_perm(&os, &agent);
        assert!(matches!(
            res,
            PermissionEvalResult::Deny(ref deny_list)
                if deny_list.iter().filter(|p| *p == DENIED_PATH_OR_FILE_GLOB).collect::<Vec<_>>().len() == 2
                && deny_list.iter().filter(|p| *p == DENIED_PATH_OR_FILE).collect::<Vec<_>>().len() == 2
        ));
    }
}
