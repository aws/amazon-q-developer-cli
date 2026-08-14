use std::collections::HashMap;
use std::path::{
    Path,
    PathBuf,
};
use std::sync::{
    Arc,
    LazyLock,
    Mutex,
};
use std::time::{
    Duration,
    SystemTime,
};

use async_trait::async_trait;
use slack_morphism::prelude::SlackFile;
use tokio::io::AsyncWriteExt;
use tracing::{
    error,
    info,
    warn,
};
use uuid::Uuid;

use super::SlackFrontend;
use super::delivery::{
    DeliveryKey,
    track_attachments,
};
use crate::engine::attachment_read::{
    AttachmentReadAuthorizer,
    AttachmentReadLease,
    DEFAULT_ATTACHMENT_ROOT,
};
use crate::engine::core::{
    Frontend,
    PromptPreparation,
    Reply,
};

const DEFAULT_MAX_FILE_BYTES: u64 = 10 * 1024 * 1024;
const DEFAULT_MAX_STORAGE_BYTES: u64 = 100 * 1024 * 1024;
const DEFAULT_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_STALE_ROOT_AGE: Duration = Duration::from_secs(30 * 60);
const MAX_ATTACHMENTS_PER_MESSAGE: usize = 10;
const REQUEST_ROOT_PREFIX: &str = "request-";

#[derive(Clone)]
pub(super) struct DownloadConfig {
    base_dir: PathBuf,
    max_file_bytes: u64,
    max_storage_bytes: u64,
    timeout: Duration,
    stale_root_age: Duration,
}

impl Default for DownloadConfig {
    fn default() -> Self {
        Self {
            base_dir: PathBuf::from(DEFAULT_ATTACHMENT_ROOT),
            max_file_bytes: DEFAULT_MAX_FILE_BYTES,
            max_storage_bytes: DEFAULT_MAX_STORAGE_BYTES,
            timeout: DEFAULT_DOWNLOAD_TIMEOUT,
            stale_root_age: DEFAULT_STALE_ROOT_AGE,
        }
    }
}

#[derive(Clone)]
pub(super) struct AttachmentStore {
    config: Arc<DownloadConfig>,
    worker_id: Arc<str>,
    read_authorizer: Arc<AttachmentReadAuthorizer>,
}

impl AttachmentStore {
    pub(super) fn new_default(read_authorizer: Arc<AttachmentReadAuthorizer>) -> anyhow::Result<Self> {
        let config = DownloadConfig {
            base_dir: read_authorizer.base_dir().to_path_buf(),
            ..DownloadConfig::default()
        };
        Self::new_with_authorizer(config, read_authorizer)
    }

    #[cfg(test)]
    fn new(config: DownloadConfig) -> anyhow::Result<Self> {
        let read_authorizer = Arc::new(AttachmentReadAuthorizer::new(config.base_dir.clone()));
        Self::new_with_authorizer(config, read_authorizer)
    }

    fn new_with_authorizer(
        config: DownloadConfig,
        read_authorizer: Arc<AttachmentReadAuthorizer>,
    ) -> anyhow::Result<Self> {
        anyhow::ensure!(
            config.base_dir == read_authorizer.base_dir(),
            "attachment store and read authorizer must use the same base directory"
        );
        ensure_private_directory(&config.base_dir)?;
        scavenge_request_roots(&config.base_dir, SystemTime::now(), config.stale_root_age)?;
        Ok(Self {
            config: Arc::new(config),
            worker_id: Uuid::new_v4().as_simple().to_string().into(),
            read_authorizer,
        })
    }

    fn create_request_root(&self, now: SystemTime) -> anyhow::Result<PathBuf> {
        let created = now.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_secs();
        let request_id = Uuid::new_v4().as_simple().to_string();
        let root = self.config.base_dir.join(format!(
            "{REQUEST_ROOT_PREFIX}{created:016x}-{}-{request_id}",
            self.worker_id
        ));
        create_private_directory(&root)?;
        Ok(root)
    }
}

#[derive(Default)]
pub(super) struct AttachmentFiles {
    root: Option<PathBuf>,
    pub(super) paths: Vec<PathBuf>,
    read_lease: Option<AttachmentReadLease>,
}

impl AttachmentFiles {
    fn new(root: PathBuf) -> Self {
        Self {
            root: Some(root),
            paths: Vec::new(),
            read_lease: None,
        }
    }

    fn activate(&mut self, authorizer: &Arc<AttachmentReadAuthorizer>, conversation: &str) -> std::io::Result<()> {
        let root = self
            .root
            .as_deref()
            .ok_or_else(|| std::io::Error::other("attachment root is unavailable"))?;
        self.read_lease = Some(authorizer.activate(conversation, root)?);
        Ok(())
    }

    #[cfg(test)]
    fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    #[cfg(test)]
    pub(super) fn for_test(root: PathBuf, path: PathBuf) -> Self {
        Self {
            root: Some(root),
            paths: vec![path],
            read_lease: None,
        }
    }
}

impl Drop for AttachmentFiles {
    fn drop(&mut self) {
        self.read_lease.take();
        let Some(root) = self.root.take() else {
            return;
        };
        if let Err(error) = std::fs::remove_dir_all(&root)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            warn!(path = %root.display(), %error, "Failed to clean up Slack attachment root");
        }
    }
}

#[derive(Default)]
pub(super) struct Attachments {
    pub(super) prompt_prefix: String,
    pub(super) files: AttachmentFiles,
    pub(super) refusals: Vec<String>,
}

impl Attachments {
    fn with_root(root: PathBuf) -> Self {
        Self {
            files: AttachmentFiles::new(root),
            ..Self::default()
        }
    }

    fn notice(&self) -> Option<String> {
        (!self.refusals.is_empty()).then(|| {
            format!(
                "Skipped {} attachment(s):\n{}",
                self.refusals.len(),
                self.refusals.join("\n")
            )
        })
    }
}

static STORAGE_RESERVATIONS: LazyLock<Mutex<HashMap<PathBuf, u64>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

struct StorageReservation {
    base_dir: PathBuf,
    bytes: u64,
}

impl Drop for StorageReservation {
    fn drop(&mut self) {
        let mut reservations = STORAGE_RESERVATIONS.lock().unwrap();
        let remaining = reservations
            .get(&self.base_dir)
            .copied()
            .unwrap_or_default()
            .saturating_sub(self.bytes);
        if remaining == 0 {
            reservations.remove(&self.base_dir);
        } else {
            reservations.insert(self.base_dir.clone(), remaining);
        }
    }
}

fn ensure_private_directory(path: &Path) -> std::io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() => {
            return Err(std::io::Error::other(format!(
                "{} is not a private directory",
                path.display()
            )));
        },
        Ok(_) => {},
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => create_private_directory(path)?,
        Err(error) => return Err(error),
    }
    set_directory_permissions(path)
}

fn create_private_directory(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new().mode(0o700).create(path)?;
    }
    #[cfg(not(unix))]
    std::fs::create_dir(path)?;
    set_directory_permissions(path)
}

fn set_directory_permissions(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

async fn open_private_file(path: &Path) -> std::io::Result<tokio::fs::File> {
    let mut options = tokio::fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        options.mode(0o600);
    }
    options.open(path).await
}

fn request_root_created_at(name: &str) -> Option<u64> {
    let suffix = name.strip_prefix(REQUEST_ROOT_PREFIX)?;
    let encoded = suffix.split('-').next()?;
    u64::from_str_radix(encoded, 16).ok()
}

fn scavenge_request_roots(base_dir: &Path, now: SystemTime, stale_age: Duration) -> std::io::Result<usize> {
    let now = now.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_secs();
    let mut removed = 0;
    for entry in std::fs::read_dir(base_dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(created) = name.to_str().and_then(request_root_created_at) else {
            continue;
        };
        if now.saturating_sub(created) < stale_age.as_secs() {
            continue;
        }
        let metadata = std::fs::symlink_metadata(entry.path())?;
        if metadata.file_type().is_symlink() {
            std::fs::remove_file(entry.path())?;
        } else if metadata.is_dir() {
            std::fs::remove_dir_all(entry.path())?;
        } else {
            continue;
        }
        removed += 1;
    }
    Ok(removed)
}

fn dir_bytes(dir: &Path) -> u64 {
    let mut total = 0_u64;
    let mut pending = vec![dir.to_path_buf()];
    while let Some(current) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(current) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                pending.push(entry.path());
            } else if file_type.is_file()
                && let Ok(metadata) = entry.metadata()
            {
                total = total.saturating_add(metadata.len());
            }
        }
    }
    total
}

fn reserve_storage(config: &DownloadConfig) -> Option<(StorageReservation, u64)> {
    let mut reservations = STORAGE_RESERVATIONS.lock().unwrap();
    let reserved = reservations.get(&config.base_dir).copied().unwrap_or_default();
    let used = dir_bytes(&config.base_dir).saturating_add(reserved);
    let available = config.max_storage_bytes.saturating_sub(used);
    let cap = available.min(config.max_file_bytes);
    if cap == 0 {
        return None;
    }
    reservations.insert(config.base_dir.clone(), reserved.saturating_add(cap));
    Some((
        StorageReservation {
            base_dir: config.base_dir.clone(),
            bytes: cap,
        },
        cap,
    ))
}

fn safe_file_name(id: &str, name: &str) -> String {
    fn alphanumeric(text: &str, limit: usize) -> String {
        text.chars().filter(char::is_ascii_alphanumeric).take(limit).collect()
    }

    let stem = alphanumeric(id, 64);
    let stem = if stem.is_empty() { "file" } else { &stem };
    let extension = alphanumeric(name.rsplit('.').next().unwrap_or_default(), 16);
    let extension = if extension.is_empty() { "bin" } else { &extension };
    format!("{stem}-{}.{}", Uuid::new_v4().as_simple(), extension)
}

fn safe_label(name: &str) -> String {
    let label: String = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_' | ' ') {
                character
            } else {
                '_'
            }
        })
        .take(80)
        .collect();
    if label.trim().is_empty() {
        "attachment".to_string()
    } else {
        label
    }
}

fn safe_mime(mime: Option<&slack_morphism::prelude::SlackMimeType>) -> String {
    let mime = mime.map(|value| value.0.as_str()).unwrap_or("application/octet-stream");
    let safe: String = mime
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || matches!(character, '/' | '+' | '-' | '.'))
        .take(100)
        .collect();
    if safe.is_empty() {
        "application/octet-stream".to_string()
    } else {
        safe
    }
}

enum DownloadError {
    TooLarge,
    Failed(anyhow::Error),
}

async fn write_capped(destination: &Path, mut response: reqwest::Response, cap: u64) -> Result<(), DownloadError> {
    let mut output = open_private_file(destination)
        .await
        .map_err(|error| DownloadError::Failed(error.into()))?;
    let mut written = 0_u64;
    let result = loop {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                written = written.saturating_add(chunk.len() as u64);
                if written > cap {
                    break Err(DownloadError::TooLarge);
                }
                if let Err(error) = output.write_all(&chunk).await {
                    break Err(DownloadError::Failed(error.into()));
                }
            },
            Ok(None) => break Ok(()),
            Err(error) => break Err(DownloadError::Failed(error.into())),
        }
    };
    if result.is_err() {
        let _ = tokio::fs::remove_file(destination).await;
    }
    result
}

fn refusal(name: &str, reason: &str) -> String {
    format!("- `{}`: {reason}", safe_label(name))
}

fn trusted_slack_file_url(url: &reqwest::Url) -> bool {
    url.scheme() == "https"
        && matches!(url.host_str(), Some("files.slack.com" | "files-origin.slack.com"))
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
}

fn attachment_client(config: &DownloadConfig) -> anyhow::Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(config.timeout)
        .redirect(reqwest::redirect::Policy::none())
        .build()?)
}

async fn download_slack_files_with(
    files: &[SlackFile],
    bot_token: &str,
    store: &AttachmentStore,
    client: &reqwest::Client,
    url_allowed: impl Fn(&reqwest::Url) -> bool,
) -> Attachments {
    let root = match store.create_request_root(SystemTime::now()) {
        Ok(root) => root,
        Err(error) => {
            error!(dir = %store.config.base_dir.display(), %error, "Failed to create Slack attachment root");
            return Attachments::default();
        },
    };
    let mut attachments = Attachments::with_root(root.clone());

    for file in files.iter().take(MAX_ATTACHMENTS_PER_MESSAGE) {
        let name = file.name.as_deref().unwrap_or("attachment");
        let Some(url) = file.url_private_download.as_ref().or(file.url_private.as_ref()) else {
            attachments.refusals.push(refusal(name, "download URL is unavailable"));
            continue;
        };
        if !url_allowed(url) {
            warn!(file_id = %file.id.0, host = ?url.host_str(), "Rejected untrusted Slack attachment URL");
            attachments
                .refusals
                .push(refusal(name, "download URL is not a trusted Slack file host"));
            continue;
        }
        let Some((_reservation, cap)) = reserve_storage(&store.config) else {
            attachments
                .refusals
                .push(refusal(name, "attachment storage is temporarily full"));
            continue;
        };

        let response = match client
            .get(url.as_str())
            .header("Authorization", format!("Bearer {bot_token}"))
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
        {
            Ok(response) => response,
            Err(error) => {
                warn!(file_id = %file.id.0, %error, "Slack attachment download failed");
                attachments.refusals.push(refusal(name, "download failed"));
                continue;
            },
        };
        if response.content_length().is_some_and(|length| length > cap) {
            attachments.refusals.push(refusal(name, "file exceeds the size limit"));
            continue;
        }

        let destination = root.join(safe_file_name(&file.id.0, name));
        match write_capped(&destination, response, cap).await {
            Ok(()) => {
                let mime = safe_mime(file.mimetype.as_ref());
                info!(path = %destination.display(), %mime, "Downloaded Slack attachment");
                attachments.prompt_prefix.push_str(&format!(
                    "[Untrusted Slack attachment ({mime}): {}. Read only if needed.]\n",
                    destination.display()
                ));
                attachments.files.paths.push(destination);
            },
            Err(DownloadError::TooLarge) => {
                attachments.refusals.push(refusal(name, "file exceeds the size limit"));
            },
            Err(DownloadError::Failed(error)) => {
                warn!(file_id = %file.id.0, %error, "Failed to store Slack attachment");
                attachments.refusals.push(refusal(name, "download failed"));
            },
        }
    }

    if files.len() > MAX_ATTACHMENTS_PER_MESSAGE {
        attachments.refusals.push(format!(
            "- {} additional files: attachment count limit is {MAX_ATTACHMENTS_PER_MESSAGE}",
            files.len() - MAX_ATTACHMENTS_PER_MESSAGE
        ));
    }
    attachments
}

async fn download_slack_files(files: &[SlackFile], bot_token: &str, store: &AttachmentStore) -> Attachments {
    let client = match attachment_client(&store.config) {
        Ok(client) => client,
        Err(error) => {
            error!(%error, "Failed to create Slack attachment client");
            return Attachments::default();
        },
    };
    download_slack_files_with(files, bot_token, store, &client, trusted_slack_file_url).await
}

struct SlackAttachmentPreparation {
    files: Vec<SlackFile>,
    frontend: Arc<SlackFrontend>,
    key: DeliveryKey,
    conversation: String,
}

#[async_trait]
impl PromptPreparation for SlackAttachmentPreparation {
    async fn prepare(self: Box<Self>, text: &mut String) {
        let mut attachments = download_slack_files(
            &self.files,
            self.frontend.bot_token.token_value.0.as_str(),
            &self.frontend.attachment_store,
        )
        .await;
        if !attachments.files.paths.is_empty()
            && let Err(error) = attachments
                .files
                .activate(&self.frontend.attachment_store.read_authorizer, &self.conversation)
        {
            error!(%error, conversation = %self.conversation, "Failed to confine Slack attachment reads");
            attachments.prompt_prefix.clear();
            attachments
                .refusals
                .push("- attachments: secure read authorization failed".into());
            attachments.files = AttachmentFiles::default();
        }
        if !attachments.prompt_prefix.is_empty() {
            *text = format!("{}{text}", attachments.prompt_prefix);
        }
        if let Some(notice) = attachments.notice() {
            let _ = self
                .frontend
                .send(Reply::Send {
                    conversation: self.key.channel.clone(),
                    reply_to: self.key.thread_ts.clone(),
                    text: notice,
                })
                .await;
        }
        track_attachments(self.key, attachments.files);
    }
}

pub(super) fn attachment_preparation(
    files: Option<&[SlackFile]>,
    frontend: Arc<SlackFrontend>,
    channel: String,
    reply_to: Option<String>,
    conversation: String,
) -> Option<Box<dyn PromptPreparation>> {
    let files = files.filter(|files| !files.is_empty())?.to_vec();
    Some(Box::new(SlackAttachmentPreparation {
        files,
        frontend,
        key: DeliveryKey::new(channel, reply_to),
        conversation,
    }))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{
        AtomicUsize,
        Ordering,
    };

    use axum::Router;
    use axum::response::Redirect;
    use axum::routing::get;
    use slack_morphism::prelude::{
        SlackFileFlags,
        SlackFileId,
        SlackMimeType,
    };

    use super::*;

    async fn serve_bytes(body_len: usize, delay: Duration) -> std::net::SocketAddr {
        let app = Router::new().route(
            "/f/:name",
            get(move || async move {
                if !delay.is_zero() {
                    tokio::time::sleep(delay).await;
                }
                let chunks: Vec<std::io::Result<Vec<u8>>> = (0..body_len)
                    .step_by(1_024)
                    .map(|start| Ok(vec![b'x'; (body_len - start).min(1_024)]))
                    .collect();
                axum::body::Body::from_stream(futures::stream::iter(chunks))
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        address
    }

    fn slack_file(id: &str, name: &str, address: std::net::SocketAddr) -> SlackFile {
        SlackFile {
            id: SlackFileId(id.to_string()),
            created: None,
            timestamp: None,
            name: Some(name.to_string()),
            title: None,
            mimetype: Some(SlackMimeType("image/png".into())),
            filetype: None,
            pretty_type: None,
            external_type: None,
            user: None,
            username: None,
            url_private: Some(format!("http://{address}/f/{id}").parse().unwrap()),
            url_private_download: None,
            permalink: None,
            permalink_public: None,
            reactions: None,
            flags: SlackFileFlags {
                editable: None,
                is_external: None,
                is_public: None,
                public_url_shared: None,
                display_as_bot: None,
                is_starred: None,
                has_rich_preview: None,
            },
        }
    }

    fn download_config(directory: &Path, max_file_bytes: u64, max_storage_bytes: u64) -> DownloadConfig {
        DownloadConfig {
            base_dir: directory.to_path_buf(),
            max_file_bytes,
            max_storage_bytes,
            timeout: Duration::from_secs(2),
            stale_root_age: DEFAULT_STALE_ROOT_AGE,
        }
    }

    fn test_store(directory: &Path, max_file_bytes: u64, max_storage_bytes: u64) -> AttachmentStore {
        AttachmentStore::new(download_config(directory, max_file_bytes, max_storage_bytes)).unwrap()
    }

    async fn download_test_files(files: &[SlackFile], store: &AttachmentStore) -> Attachments {
        let client = attachment_client(&store.config).unwrap();
        download_slack_files_with(files, "token", store, &client, |_| true).await
    }

    #[test]
    fn attachment_urls_require_https_slack_file_hosts() {
        for allowed in [
            "https://files.slack.com/files-pri/T1-F1/file.png",
            "https://files-origin.slack.com/files-pri/T1-F1/file.png",
        ] {
            assert!(trusted_slack_file_url(&allowed.parse().unwrap()), "{allowed}");
        }
        for rejected in [
            "http://files.slack.com/files-pri/T1-F1/file.png",
            "https://127.0.0.1/file.png",
            "https://localhost/file.png",
            "https://files.slack.com.evil.example/file.png",
            "https://slack.com/file.png",
            "https://user@files.slack.com/file.png",
            "https://files.slack.com:8443/file.png",
        ] {
            assert!(!trusted_slack_file_url(&rejected.parse().unwrap()), "{rejected}");
        }
    }

    #[tokio::test]
    async fn untrusted_attachment_urls_are_rejected_without_fetching() {
        let directory = tempfile::tempdir().unwrap();
        let address = serve_bytes(1_024, Duration::ZERO).await;
        let store = test_store(directory.path(), 8 * 1_024, 20 * 1_024);

        let attachments =
            download_slack_files(&[slack_file("loopback", "loopback.png", address)], "token", &store).await;

        assert!(attachments.files.paths.is_empty());
        assert_eq!(attachments.refusals.len(), 1);
        let root = attachments.files.root().unwrap().to_path_buf();
        drop(attachments);
        assert!(!root.exists());
    }

    #[tokio::test]
    async fn attachment_client_does_not_follow_redirects() {
        let target_hits = Arc::new(AtomicUsize::new(0));
        let hits = target_hits.clone();
        let app = Router::new()
            .route("/start", get(|| async { Redirect::temporary("/target") }))
            .route(
                "/target",
                get(move || {
                    let hits = hits.clone();
                    async move {
                        hits.fetch_add(1, Ordering::Relaxed);
                        "secret"
                    }
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let directory = tempfile::tempdir().unwrap();
        let store = test_store(directory.path(), 1_024, 1_024);

        let response = attachment_client(&store.config)
            .unwrap()
            .get(format!("http://{address}/start"))
            .send()
            .await
            .unwrap();

        assert!(response.status().is_redirection());
        assert_eq!(target_hits.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn oversized_attachments_are_stream_capped_without_partials() {
        let directory = tempfile::tempdir().unwrap();
        let address = serve_bytes(64 * 1_024, Duration::ZERO).await;
        let store = test_store(directory.path(), 8 * 1_024, 1024 * 1_024);

        let attachments = download_test_files(&[slack_file("large", "large.png", address)], &store).await;

        assert!(attachments.prompt_prefix.is_empty());
        assert_eq!(attachments.refusals.len(), 1);
        assert_eq!(dir_bytes(directory.path()), 0);
        let root = attachments.files.root().unwrap().to_path_buf();
        drop(attachments);
        assert!(!root.exists());
    }

    #[tokio::test]
    async fn aggregate_attachment_budget_refuses_excess_files() {
        let directory = tempfile::tempdir().unwrap();
        let address = serve_bytes(8 * 1_024, Duration::ZERO).await;
        let store = test_store(directory.path(), 8 * 1_024, 20 * 1_024);
        let files = ["a", "b", "c", "d"].map(|id| slack_file(id, &format!("{id}.png"), address));

        let attachments = download_test_files(&files, &store).await;

        assert_eq!(attachments.files.paths.len(), 2);
        assert_eq!(attachments.refusals.len(), 2);
        assert!(dir_bytes(directory.path()) <= store.config.max_storage_bytes);
    }

    #[tokio::test]
    async fn attachment_downloads_time_out_and_leave_no_file() {
        let directory = tempfile::tempdir().unwrap();
        let address = serve_bytes(1_024, Duration::from_millis(200)).await;
        let mut config = download_config(directory.path(), 8 * 1_024, 20 * 1_024);
        config.timeout = Duration::from_millis(25);
        let store = AttachmentStore::new(config).unwrap();

        let attachments = download_test_files(&[slack_file("slow", "slow.png", address)], &store).await;

        assert!(attachments.files.paths.is_empty());
        assert_eq!(attachments.refusals.len(), 1);
        assert_eq!(dir_bytes(directory.path()), 0);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn request_roots_and_files_are_private_and_isolated() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let address = serve_bytes(1_024, Duration::ZERO).await;
        let store = test_store(directory.path(), 8 * 1_024, 20 * 1_024);
        let first = download_test_files(&[slack_file("one", "one.png", address)], &store).await;
        let second = download_test_files(&[slack_file("two", "two.png", address)], &store).await;
        let first_root = first.files.root().unwrap();
        let second_root = second.files.root().unwrap();

        assert_ne!(first_root, second_root);
        assert_eq!(
            std::fs::metadata(first_root).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(second_root).unwrap().permissions().mode() & 0o777,
            0o700
        );
        for attachments in [&first, &second] {
            for path in &attachments.files.paths {
                assert_eq!(path.parent(), attachments.files.root());
                assert_eq!(std::fs::metadata(path).unwrap().permissions().mode() & 0o777, 0o600);
                assert!(attachments.prompt_prefix.contains(path.to_str().unwrap()));
            }
        }
        assert!(!first.prompt_prefix.contains(second_root.to_str().unwrap()));
        assert!(!second.prompt_prefix.contains(first_root.to_str().unwrap()));
    }

    #[tokio::test]
    async fn attachment_guard_removes_files_and_request_root_on_drop() {
        let directory = tempfile::tempdir().unwrap();
        let address = serve_bytes(1_024, Duration::ZERO).await;
        let store = test_store(directory.path(), 8 * 1_024, 20 * 1_024);
        let mut attachments = download_test_files(&[slack_file("keep", "keep.png", address)], &store).await;
        attachments
            .files
            .activate(&store.read_authorizer, "thread:C1:1")
            .unwrap();
        let root = attachments.files.root().unwrap().to_path_buf();
        let paths = attachments.files.paths.clone();
        assert_eq!(paths.len(), 1);
        assert!(paths[0].exists());
        assert_eq!(
            store
                .read_authorizer
                .evaluate("thread:C1:1", &[paths[0].display().to_string()]),
            crate::engine::attachment_read::AttachmentReadDecision::Allow
        );

        drop(attachments);

        assert!(!paths[0].exists());
        assert!(!root.exists());
        assert_eq!(
            store
                .read_authorizer
                .evaluate("thread:C1:1", &[paths[0].display().to_string()]),
            crate::engine::attachment_read::AttachmentReadDecision::Deny
        );
    }

    #[test]
    fn startup_scavenges_only_stale_request_roots() {
        let directory = tempfile::tempdir().unwrap();
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(10_000);
        let stale = directory
            .path()
            .join(format!("{REQUEST_ROOT_PREFIX}{:016x}-worker-old", 1_000));
        let fresh = directory
            .path()
            .join(format!("{REQUEST_ROOT_PREFIX}{:016x}-worker-new", 9_900));
        let unrelated = directory.path().join("keep-me");
        std::fs::create_dir(&stale).unwrap();
        std::fs::write(stale.join("secret"), b"old").unwrap();
        std::fs::create_dir(&fresh).unwrap();
        std::fs::create_dir(&unrelated).unwrap();

        let removed = scavenge_request_roots(directory.path(), now, Duration::from_secs(300)).unwrap();

        assert_eq!(removed, 1);
        assert!(!stale.exists());
        assert!(fresh.exists());
        assert!(unrelated.exists());
    }

    #[test]
    fn attachment_names_cannot_escape_the_request_root() {
        for (id, name) in [
            ("F1", "normal.png"),
            ("../../etc/passwd", "normal.png"),
            ("F1", "../../../root/.ssh/authorized_keys"),
            ("", ""),
        ] {
            let root = Path::new("/tmp/downloads/request");
            let destination = root.join(safe_file_name(id, name));
            assert_eq!(destination.parent(), Some(root));
        }
        assert!(!safe_label("<!channel>\n`name`").contains('<'));
    }
}
