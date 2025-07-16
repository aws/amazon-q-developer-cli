use std::collections::HashMap;
use std::io::Write;
use std::path::{
    Path,
    PathBuf,
};
use std::sync::Arc;
use std::time::Duration;

use crossterm::queue;
use crossterm::style::{
    self,
    Color,
};
use eyre::{
    Result,
    eyre,
};
use globset::{
    Glob,
    GlobSetBuilder,
};
use notify::{
    RecommendedWatcher,
    RecursiveMode,
};
use notify_debouncer_mini::{
    DebounceEventResult,
    Debouncer,
    new_debouncer,
};
use serde::{
    Deserialize,
    Serialize,
};
use tokio::sync::{
    RwLock,
    mpsc,
};
use tracing::{
    error,
    warn,
};

use super::{
    InvokeOutput,
    OutputKind,
    format_path,
    queue_function_result,
    sanitize_path_tool_arg,
};
use crate::cli::agent::{
    Agent,
    PermissionEvalResult,
};
use crate::os::Os;

type WatcherMap = Arc<RwLock<HashMap<PathBuf, Debouncer<RecommendedWatcher>>>>;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FsWatch {
    pub operation: WatchOperation,
    #[serde(skip)]
    pub watchers: Option<WatcherMap>,
    #[serde(skip)]
    pub event_sender: Option<mpsc::UnboundedSender<WatchEvent>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WatchOperation {
    Add {
        paths: Vec<String>,
        #[serde(default = "default_recursive")]
        recursive: bool,
        #[serde(default = "default_debounce_ms")]
        debounce_ms: u64,
    },
    Remove {
        paths: Vec<String>,
    },
    List,
    Stop,
}

#[derive(Debug, Clone)]
pub struct WatchEvent {
    pub path: PathBuf,
    pub event_type: String,
    pub timestamp: std::time::SystemTime,
}

impl Default for FsWatch {
    fn default() -> Self {
        Self {
            operation: WatchOperation::List,
            watchers: None,
            event_sender: None,
        }
    }
}

fn default_recursive() -> bool {
    true
}

fn default_debounce_ms() -> u64 {
    500
}

impl FsWatch {
    pub fn new(operation: WatchOperation) -> Self {
        Self {
            operation,
            watchers: Some(Arc::new(RwLock::new(HashMap::new()))),
            event_sender: None,
        }
    }

    pub fn set_event_sender(&mut self, sender: mpsc::UnboundedSender<WatchEvent>) {
        self.event_sender = Some(sender);
    }

    pub fn eval_perm(&self, agent: &Agent) -> PermissionEvalResult {
        #[derive(Debug, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Settings {
            #[serde(default)]
            allowed_paths: Vec<String>,
            #[serde(default)]
            denied_paths: Vec<String>,
        }

        let is_in_allowlist = agent.allowed_tools.contains("fs_watch");
        match agent.tools_settings.get("fs_watch") {
            Some(settings) if is_in_allowlist => {
                let Settings {
                    allowed_paths,
                    denied_paths,
                } = match serde_json::from_value::<Settings>(settings.clone()) {
                    Ok(settings) => settings,
                    Err(e) => {
                        error!("Failed to deserialize tool settings for fs_watch: {:?}", e);
                        return PermissionEvalResult::Ask;
                    },
                };

                let allow_set = {
                    let mut builder = GlobSetBuilder::new();
                    for path in &allowed_paths {
                        if let Ok(glob) = Glob::new(path) {
                            builder.add(glob);
                        } else {
                            warn!("Failed to create glob from path given: {path}. Ignoring.");
                        }
                    }
                    builder.build()
                };

                let deny_set = {
                    let mut builder = GlobSetBuilder::new();
                    for path in &denied_paths {
                        if let Ok(glob) = Glob::new(path) {
                            builder.add(glob);
                        } else {
                            warn!("Failed to create glob from path given: {path}. Ignoring.");
                        }
                    }
                    builder.build()
                };

                match (allow_set, deny_set) {
                    (Ok(allow_set), Ok(deny_set)) => match &self.operation {
                        WatchOperation::Add { paths, .. } => {
                            for path_str in paths {
                                let path = Path::new(path_str);
                                if deny_set.is_match(path) {
                                    return PermissionEvalResult::Deny;
                                }
                                if !allow_set.is_match(path) {
                                    return PermissionEvalResult::Ask;
                                }
                            }
                            PermissionEvalResult::Allow
                        },
                        WatchOperation::Remove { .. } | WatchOperation::List | WatchOperation::Stop => {
                            PermissionEvalResult::Allow
                        },
                    },
                    (allow_res, deny_res) => {
                        if let Err(e) = allow_res {
                            warn!("fs_watch failed to build allow set: {:?}", e);
                        }
                        if let Err(e) = deny_res {
                            warn!("fs_watch failed to build deny set: {:?}", e);
                        }
                        PermissionEvalResult::Ask
                    },
                }
            },
            Some(_) => PermissionEvalResult::Deny,
            None if is_in_allowlist => PermissionEvalResult::Ask,
            None => PermissionEvalResult::Deny,
        }
    }

    pub async fn validate(&mut self, os: &Os) -> Result<()> {
        match &mut self.operation {
            WatchOperation::Add { paths, .. } => {
                for path_str in paths {
                    let path = sanitize_path_tool_arg(os, &*path_str);
                    if !os.fs.exists(&path) {
                        return Err(eyre!("Path does not exist: {}", path.display()));
                    }
                    *path_str = path.to_string_lossy().to_string();
                }
            },
            WatchOperation::Remove { paths } => {
                for path_str in paths {
                    let path = sanitize_path_tool_arg(os, &*path_str);
                    *path_str = path.to_string_lossy().to_string();
                }
            },
            _ => {},
        }
        Ok(())
    }

    pub fn queue_description(&self, output: &mut impl Write) -> Result<()> {
        let description = match &self.operation {
            WatchOperation::Add { paths, recursive, .. } => {
                let mode = if *recursive { "recursively" } else { "non-recursively" };
                format!("Watch {} {} for filesystem changes", paths.join(", "), mode)
            },
            WatchOperation::Remove { paths } => {
                format!("Stop watching {} for filesystem changes", paths.join(", "))
            },
            WatchOperation::List => "List currently watched paths".to_string(),
            WatchOperation::Stop => "Stop all filesystem watching".to_string(),
        };

        queue!(
            output,
            style::SetForegroundColor(Color::Blue),
            style::Print("🔍 "),
            style::ResetColor,
            style::Print(&description),
            style::Print("\n"),
        )?;

        Ok(())
    }

    pub async fn invoke(&self, os: &Os, stdout: &mut impl Write) -> Result<InvokeOutput> {
        let watchers = self
            .watchers
            .as_ref()
            .ok_or_else(|| eyre!("Watchers not initialized"))?;

        match &self.operation {
            WatchOperation::Add {
                paths,
                recursive,
                debounce_ms,
            } => {
                self.add_watchers(os, stdout, watchers, paths, *recursive, *debounce_ms)
                    .await
            },
            WatchOperation::Remove { paths } => self.remove_watchers(os, stdout, watchers, paths).await,
            WatchOperation::List => self.list_watchers(os, stdout, watchers).await,
            WatchOperation::Stop => self.stop_all_watchers(stdout, watchers).await,
        }
    }

    async fn add_watchers(
        &self,
        os: &Os,
        stdout: &mut impl Write,
        watchers: &WatcherMap,
        paths: &[String],
        recursive: bool,
        debounce_ms: u64,
    ) -> Result<InvokeOutput> {
        let mut added_paths = Vec::new();
        let mut errors = Vec::new();

        for path_str in paths {
            let path = PathBuf::from(path_str);
            let canonical_path = match os.fs.canonicalize(&path).await {
                Ok(p) => p,
                Err(e) => {
                    errors.push(format!("Failed to canonicalize {}: {}", path.display(), e));
                    continue;
                },
            };

            // Check if already watching this path
            {
                let watchers_lock = watchers.read().await;
                if watchers_lock.contains_key(&canonical_path) {
                    errors.push(format!("Already watching {}", canonical_path.display()));
                    continue;
                }
            }

            // Create debounced watcher
            let event_sender = self.event_sender.clone();
            let watch_path = canonical_path.clone();

            let debouncer = new_debouncer(Duration::from_millis(debounce_ms), move |res: DebounceEventResult| {
                if let Some(sender) = &event_sender {
                    match res {
                        Ok(events) => {
                            for event in events {
                                let watch_event = WatchEvent {
                                    path: event.path.clone(),
                                    event_type: format!("{:?}", event.kind),
                                    timestamp: std::time::SystemTime::now(),
                                };
                                let _ = sender.send(watch_event);
                            }
                        },
                        Err(e) => {
                            let error_event = WatchEvent {
                                path: watch_path.clone(),
                                event_type: format!("Error: {}", e),
                                timestamp: std::time::SystemTime::now(),
                            };
                            let _ = sender.send(error_event);
                        },
                    }
                }
            });

            match debouncer {
                Ok(mut debouncer) => {
                    let mode = if recursive {
                        RecursiveMode::Recursive
                    } else {
                        RecursiveMode::NonRecursive
                    };

                    match debouncer.watcher().watch(&canonical_path, mode) {
                        Ok(_) => {
                            {
                                let mut watchers_lock = watchers.write().await;
                                watchers_lock.insert(canonical_path.clone(), debouncer);
                            }
                            added_paths.push(canonical_path);
                        },
                        Err(e) => {
                            errors.push(format!("Failed to watch {}: {}", canonical_path.display(), e));
                        },
                    }
                },
                Err(e) => {
                    errors.push(format!(
                        "Failed to create watcher for {}: {}",
                        canonical_path.display(),
                        e
                    ));
                },
            }
        }

        let mut result = String::new();

        if !added_paths.is_empty() {
            result.push_str("Successfully started watching:\n");
            for path in &added_paths {
                let formatted_path = format_path(os.env.current_dir()?, path);
                result.push_str(&format!("  • {}\n", formatted_path));
            }
        }

        if !errors.is_empty() {
            if !result.is_empty() {
                result.push('\n');
            }
            result.push_str("Errors:\n");
            for error in &errors {
                result.push_str(&format!("  • {}\n", error));
            }
        }

        if added_paths.is_empty() && errors.is_empty() {
            result = "No paths were added for watching.".to_string();
        }

        queue_function_result(&result, stdout, !errors.is_empty(), false)?;

        Ok(InvokeOutput {
            output: OutputKind::Text(result),
        })
    }

    async fn remove_watchers(
        &self,
        os: &Os,
        stdout: &mut impl Write,
        watchers: &WatcherMap,
        paths: &[String],
    ) -> Result<InvokeOutput> {
        let mut removed_paths = Vec::new();
        let mut errors = Vec::new();

        for path_str in paths {
            let path = PathBuf::from(path_str);
            let canonical_path = match os.fs.canonicalize(&path).await {
                Ok(p) => p,
                Err(_) => path, // Use original path if canonicalization fails
            };

            {
                let mut watchers_lock = watchers.write().await;
                if watchers_lock.remove(&canonical_path).is_some() {
                    removed_paths.push(canonical_path);
                } else {
                    errors.push(format!("Not watching {}", canonical_path.display()));
                }
            }
        }

        let mut result = String::new();

        if !removed_paths.is_empty() {
            result.push_str("Stopped watching:\n");
            for path in &removed_paths {
                let formatted_path = format_path(os.env.current_dir()?, path);
                result.push_str(&format!("  • {}\n", formatted_path));
            }
        }

        if !errors.is_empty() {
            if !result.is_empty() {
                result.push('\n');
            }
            result.push_str("Errors:\n");
            for error in &errors {
                result.push_str(&format!("  • {}\n", error));
            }
        }

        if removed_paths.is_empty() && errors.is_empty() {
            result = "No paths were removed from watching.".to_string();
        }

        queue_function_result(&result, stdout, !errors.is_empty(), false)?;

        Ok(InvokeOutput {
            output: OutputKind::Text(result),
        })
    }

    async fn list_watchers(&self, os: &Os, stdout: &mut impl Write, watchers: &WatcherMap) -> Result<InvokeOutput> {
        let watchers_lock = watchers.read().await;

        let result = if watchers_lock.is_empty() {
            "No paths are currently being watched.".to_string()
        } else {
            let mut result = format!("Currently watching {} path(s):\n", watchers_lock.len());
            for path in watchers_lock.keys() {
                let formatted_path = format_path(os.env.current_dir()?, path);
                result.push_str(&format!("  • {}\n", formatted_path));
            }
            result
        };

        queue_function_result(&result, stdout, false, true)?;

        Ok(InvokeOutput {
            output: OutputKind::Text(result),
        })
    }

    async fn stop_all_watchers(&self, stdout: &mut impl Write, watchers: &WatcherMap) -> Result<InvokeOutput> {
        let mut watchers_lock = watchers.write().await;
        let count = watchers_lock.len();
        watchers_lock.clear();

        let result = if count == 0 {
            "No watchers were active.".to_string()
        } else {
            format!("Stopped {} watcher(s).", count)
        };

        queue_function_result(&result, stdout, false, false)?;

        Ok(InvokeOutput {
            output: OutputKind::Text(result),
        })
    }
}
