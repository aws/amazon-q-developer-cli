use std::collections::HashMap;
use std::path::{
    Component,
    Path,
    PathBuf,
};
use std::sync::{
    Arc,
    Mutex,
};

pub const DEFAULT_ATTACHMENT_ROOT: &str = "/tmp/kiro-bot-files";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentReadDecision {
    Allow,
    Deny,
    Unmanaged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ActiveRoot {
    lexical: PathBuf,
    canonical: PathBuf,
}

pub struct AttachmentReadAuthorizer {
    base_dir: PathBuf,
    active: Mutex<HashMap<String, Vec<ActiveRoot>>>,
}

impl Default for AttachmentReadAuthorizer {
    fn default() -> Self {
        Self::new(DEFAULT_ATTACHMENT_ROOT)
    }
}

impl AttachmentReadAuthorizer {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
            active: Mutex::new(HashMap::new()),
        }
    }

    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    pub fn activate(
        self: &Arc<Self>,
        conversation: impl Into<String>,
        root: &Path,
    ) -> std::io::Result<AttachmentReadLease> {
        if !root.is_absolute() || has_ambiguous_components(root) {
            return Err(std::io::Error::other(
                "attachment root must be an absolute normalized path",
            ));
        }
        let metadata = std::fs::symlink_metadata(root)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(std::io::Error::other("attachment root must be a real directory"));
        }

        let canonical_base = std::fs::canonicalize(&self.base_dir)?;
        let canonical_root = std::fs::canonicalize(root)?;
        if canonical_root == canonical_base || !canonical_root.starts_with(&canonical_base) {
            return Err(std::io::Error::other(
                "attachment root escapes the managed base directory",
            ));
        }

        let conversation = conversation.into();
        if conversation.is_empty() {
            return Err(std::io::Error::other("attachment root requires a conversation"));
        }
        let active_root = ActiveRoot {
            lexical: root.to_path_buf(),
            canonical: canonical_root,
        };
        self.active
            .lock()
            .unwrap()
            .entry(conversation.clone())
            .or_default()
            .push(active_root.clone());
        Ok(AttachmentReadLease {
            authorizer: self.clone(),
            conversation,
            root: active_root,
        })
    }

    pub fn evaluate(&self, conversation: &str, paths: &[String]) -> AttachmentReadDecision {
        if conversation.is_empty() || paths.is_empty() {
            return AttachmentReadDecision::Deny;
        }
        let active = self.active.lock().unwrap().clone();
        let canonical_base = std::fs::canonicalize(&self.base_dir).ok();
        let mut saw_unmanaged = false;

        for raw in paths {
            match self.evaluate_path(conversation, Path::new(raw), &active, canonical_base.as_deref()) {
                AttachmentReadDecision::Allow => {},
                AttachmentReadDecision::Deny => return AttachmentReadDecision::Deny,
                AttachmentReadDecision::Unmanaged => saw_unmanaged = true,
            }
        }

        if saw_unmanaged {
            AttachmentReadDecision::Unmanaged
        } else {
            AttachmentReadDecision::Allow
        }
    }

    fn evaluate_path(
        &self,
        conversation: &str,
        path: &Path,
        active: &HashMap<String, Vec<ActiveRoot>>,
        canonical_base: Option<&Path>,
    ) -> AttachmentReadDecision {
        if !path.is_absolute() || has_ambiguous_components(path) {
            return AttachmentReadDecision::Deny;
        }

        let lexical_managed = path.starts_with(&self.base_dir);
        let Ok(canonical) = std::fs::canonicalize(path) else {
            return if lexical_managed {
                AttachmentReadDecision::Deny
            } else {
                AttachmentReadDecision::Unmanaged
            };
        };

        if let Some(roots) = active.get(conversation) {
            for root in roots {
                if path.starts_with(&root.lexical) && canonical.starts_with(&root.canonical) {
                    return if contains_symlink(&root.lexical, path) {
                        AttachmentReadDecision::Deny
                    } else {
                        AttachmentReadDecision::Allow
                    };
                }
            }
        }

        let belongs_to_active_root = active
            .values()
            .flatten()
            .any(|root| path.starts_with(&root.lexical) || canonical.starts_with(&root.canonical));
        let canonical_managed = canonical_base.is_some_and(|base| canonical.starts_with(base));
        if lexical_managed || canonical_managed || belongs_to_active_root {
            AttachmentReadDecision::Deny
        } else {
            AttachmentReadDecision::Unmanaged
        }
    }

    fn revoke(&self, conversation: &str, root: &ActiveRoot) {
        let mut active = self.active.lock().unwrap();
        let Some(roots) = active.get_mut(conversation) else {
            return;
        };
        roots.retain(|candidate| candidate != root);
        if roots.is_empty() {
            active.remove(conversation);
        }
    }
}

pub struct AttachmentReadLease {
    authorizer: Arc<AttachmentReadAuthorizer>,
    conversation: String,
    root: ActiveRoot,
}

impl Drop for AttachmentReadLease {
    fn drop(&mut self) {
        self.authorizer.revoke(&self.conversation, &self.root);
    }
}

fn has_ambiguous_components(path: &Path) -> bool {
    path.components()
        .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
}

fn contains_symlink(root: &Path, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return true;
    };
    let mut current = root.to_path_buf();
    if is_symlink_or_missing(&current) {
        return true;
    }
    for component in relative.components() {
        let Component::Normal(component) = component else {
            return true;
        };
        current.push(component);
        if is_symlink_or_missing(&current) {
            return true;
        }
    }
    false
}

fn is_symlink_or_missing(path: &Path) -> bool {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata.file_type().is_symlink(),
        Err(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request_root(base: &Path, name: &str) -> (PathBuf, PathBuf) {
        let root = base.join(name);
        std::fs::create_dir(&root).unwrap();
        let file = root.join("attachment.txt");
        std::fs::write(&file, b"private").unwrap();
        (root, file)
    }

    #[test]
    fn active_root_allows_only_its_conversation() {
        let directory = tempfile::tempdir().unwrap();
        let base = directory.path().join("attachments");
        std::fs::create_dir(&base).unwrap();
        let (first_root, first_file) = request_root(&base, "request-first");
        let (second_root, second_file) = request_root(&base, "request-second");
        let authorizer = Arc::new(AttachmentReadAuthorizer::new(&base));
        let _first = authorizer.activate("thread:C1:1", &first_root).unwrap();
        let _second = authorizer.activate("thread:C1:2", &second_root).unwrap();

        assert_eq!(
            authorizer.evaluate("thread:C1:1", &[first_file.display().to_string()]),
            AttachmentReadDecision::Allow
        );
        assert_eq!(
            authorizer.evaluate("thread:C1:1", &[second_file.display().to_string()]),
            AttachmentReadDecision::Deny
        );
    }

    #[test]
    fn traversal_and_symlink_escape_are_denied() {
        let directory = tempfile::tempdir().unwrap();
        let base = directory.path().join("attachments");
        std::fs::create_dir(&base).unwrap();
        let (root, file) = request_root(&base, "request-active");
        let outside = directory.path().join("outside.txt");
        std::fs::write(&outside, b"outside").unwrap();
        let authorizer = Arc::new(AttachmentReadAuthorizer::new(&base));
        let _lease = authorizer.activate("thread:C1:1", &root).unwrap();

        let traversal = root.join("nested").join("..").join(file.file_name().unwrap());
        assert_eq!(
            authorizer.evaluate("thread:C1:1", &[traversal.display().to_string()]),
            AttachmentReadDecision::Deny
        );

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside, root.join("escape")).unwrap();
            assert_eq!(
                authorizer.evaluate("thread:C1:1", &[root.join("escape").display().to_string()]),
                AttachmentReadDecision::Deny
            );
            std::os::unix::fs::symlink(&file, root.join("alias")).unwrap();
            assert_eq!(
                authorizer.evaluate("thread:C1:1", &[root.join("alias").display().to_string()]),
                AttachmentReadDecision::Deny
            );
        }
    }

    #[test]
    fn dropping_lease_revokes_access_before_cleanup() {
        let directory = tempfile::tempdir().unwrap();
        let base = directory.path().join("attachments");
        std::fs::create_dir(&base).unwrap();
        let (root, file) = request_root(&base, "request-active");
        let authorizer = Arc::new(AttachmentReadAuthorizer::new(&base));
        let lease = authorizer.activate("thread:C1:1", &root).unwrap();
        let path = file.display().to_string();

        assert_eq!(
            authorizer.evaluate("thread:C1:1", std::slice::from_ref(&path)),
            AttachmentReadDecision::Allow
        );
        drop(lease);
        assert_eq!(
            authorizer.evaluate("thread:C1:1", &[path]),
            AttachmentReadDecision::Deny
        );
    }
}
