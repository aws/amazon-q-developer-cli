use std::fs::{
    File,
    OpenOptions,
};
use std::io::{
    ErrorKind,
    Seek,
    SeekFrom,
    Write,
};
use std::path::{
    Path,
    PathBuf,
};
use std::sync::{
    Arc,
    Mutex,
    Once,
    OnceLock,
    Weak,
};
use std::time::{
    Duration,
    SystemTime,
    UNIX_EPOCH,
};

use kiro_telemetry::{
    MetricRecord,
    metric,
};
use serde::{
    Deserialize,
    Serialize,
};
use uuid::Uuid;

use crate::process::ProcessIdentity;

const RECEIPT_DIR: &str = "run-receipts";
const REAPER_LOCK: &str = ".reaper.lock";
const MAX_RECEIPT_BYTES: u64 = 1024;
const MAX_RECEIPTS: usize = 32;
const MAX_DIRECTORY_BYTES: u64 = 64 * 1024;
const MAX_RECEIPT_AGE: Duration = Duration::from_secs(30 * 24 * 60 * 60);

type ReceiptSlot = Arc<Mutex<Option<ActiveReceipt>>>;
type PanicReceiptRegistry = Mutex<Vec<Weak<Mutex<Option<ActiveReceipt>>>>>;
static PANIC_HOOK: Once = Once::new();
static PANIC_RECEIPTS: OnceLock<PanicReceiptRegistry> = OnceLock::new();

#[derive(Clone, Debug)]
pub struct RunReceiptStore {
    directory: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ReceiptData {
    schema_version: u8,
    run_id: Uuid,
    pid: u32,
    started_unix_millis: u64,
    version_full: String,
    engine: String,
    os_type: String,
    process_role: String,
    crash_kind: Option<String>,
    completed: bool,
}

#[derive(Debug)]
struct ActiveReceipt {
    path: PathBuf,
    file: File,
    data: ReceiptData,
}

#[derive(Debug)]
struct IncompleteReceipt {
    path: Option<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct RunReceipt {
    slot: ReceiptSlot,
}

#[derive(Debug)]
struct RecoveredReceipt {
    path: PathBuf,
    file: Option<File>,
    record: MetricRecord,
}

#[derive(Debug, Default)]
pub struct RunReceiptRecovery {
    directory: PathBuf,
    directory_lock: Option<File>,
    receipts: Vec<RecoveredReceipt>,
}

#[derive(Debug)]
pub(crate) struct DeferredRunReceiptAcknowledgement {
    directory: PathBuf,
    paths: Vec<PathBuf>,
}

impl RunReceiptStore {
    pub fn new(state_dir: impl Into<PathBuf>) -> Self {
        Self {
            directory: state_dir.into().join(RECEIPT_DIR),
        }
    }

    pub fn start(&self, identity: ProcessIdentity) -> std::io::Result<RunReceipt> {
        self.start_with_opener(identity, open_existing_private_file)
    }

    fn start_with_opener(
        &self,
        identity: ProcessIdentity,
        open_receipt: impl FnOnce(&Path) -> std::io::Result<File>,
    ) -> std::io::Result<RunReceipt> {
        std::fs::create_dir_all(&self.directory)?;
        let directory_lock = open_or_create_private_file(&self.directory.join(REAPER_LOCK))?;
        lock_file(&directory_lock, false)?;
        self.prune_unlocked()?;

        let data = ReceiptData {
            schema_version: 1,
            run_id: Uuid::new_v4(),
            pid: std::process::id(),
            started_unix_millis: now_unix_millis(),
            version_full: env!("CARGO_PKG_VERSION").to_string(),
            engine: identity.engine.as_str().to_string(),
            os_type: current_os_type().as_str().to_string(),
            process_role: identity.role.as_str().to_string(),
            crash_kind: None,
            completed: false,
        };
        let path = self.directory.join(format!("{}.json", data.run_id));
        atomic_write(&path, &data)?;
        let mut incomplete = IncompleteReceipt::new(path.clone());
        let file = open_receipt(&path)?;
        lock_file(&file, false)?;
        unlock_file(&directory_lock)?;
        incomplete.disarm();

        let slot = Arc::new(Mutex::new(Some(ActiveReceipt { path, file, data })));
        register_panic_receipt(&slot);
        Ok(RunReceipt { slot })
    }

    pub fn recover(&self) -> RunReceiptRecovery {
        let Ok(directory_lock) = open_or_create_private_file(&self.directory.join(REAPER_LOCK)) else {
            return RunReceiptRecovery::default();
        };
        if lock_file(&directory_lock, true).is_err() {
            return RunReceiptRecovery::default();
        }

        let mut receipts = Vec::new();
        for path in receipt_paths(&self.directory) {
            let Ok(file) = open_existing_private_file(&path) else {
                continue;
            };
            if lock_file(&file, true).is_err() {
                continue;
            }
            let Some(data) = read_receipt(&path) else {
                drop_locked(file);
                let _ = std::fs::remove_file(path);
                continue;
            };
            if data.completed || receipt_expired(&data) {
                drop_locked(file);
                let _ = std::fs::remove_file(path);
                continue;
            }

            let crash_kind = data
                .crash_kind
                .as_deref()
                .map_or(metric::CrashKind::UncleanExit, metric::CrashKind::from_name);
            let record = metric::record_crash_for_version(
                &data.version_full,
                metric::Engine::from_name(&data.engine),
                metric::OsType::from_name(&data.os_type),
                metric::ProcessRole::from_name(&data.process_role),
                crash_kind,
            );
            receipts.push(RecoveredReceipt {
                path,
                file: Some(file),
                record,
            });
        }

        RunReceiptRecovery {
            directory: self.directory.clone(),
            directory_lock: Some(directory_lock),
            receipts,
        }
    }

    pub fn clear_unlocked(&self) {
        let mut recovery = self.recover();
        recovery.discard();
        let _ = self.prune_unlocked();
    }

    fn prune_unlocked(&self) -> std::io::Result<()> {
        self.prune_to_limits(
            MAX_RECEIPTS.saturating_sub(1),
            MAX_DIRECTORY_BYTES.saturating_sub(MAX_RECEIPT_BYTES),
        )
    }

    fn prune_to_limits(&self, max_receipts: usize, max_directory_bytes: u64) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.directory)?;
        let mut entries = receipt_paths(&self.directory)
            .into_iter()
            .filter_map(|path| {
                let metadata = std::fs::metadata(&path).ok()?;
                Some((path, metadata.modified().unwrap_or(UNIX_EPOCH), metadata.len()))
            })
            .collect::<Vec<_>>();
        entries.sort_by_key(|(_, modified, _)| *modified);

        let mut count = entries.len();
        let mut bytes = entries.iter().map(|(_, _, size)| *size).sum::<u64>();
        for (path, _, size) in entries {
            let malformed_or_expired = read_receipt(&path).is_none_or(|data| data.completed || receipt_expired(&data));
            if !malformed_or_expired && count <= max_receipts && bytes <= max_directory_bytes {
                continue;
            }
            let Ok(file) = open_existing_private_file(&path) else {
                continue;
            };
            if lock_file(&file, true).is_err() {
                continue;
            }
            drop_locked(file);
            if std::fs::remove_file(&path).is_ok() {
                count = count.saturating_sub(1);
                bytes = bytes.saturating_sub(size);
            }
        }
        Ok(())
    }
}

impl IncompleteReceipt {
    fn new(path: PathBuf) -> Self {
        Self { path: Some(path) }
    }

    fn disarm(&mut self) {
        self.path = None;
    }
}

impl Drop for IncompleteReceipt {
    fn drop(&mut self) {
        if let Some(path) = self.path.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

impl RunReceipt {
    pub fn mark_crash(&self, crash_kind: metric::CrashKind) {
        let Ok(mut slot) = self.slot.try_lock() else {
            return;
        };
        if let Some(active) = slot.as_mut() {
            active.data.crash_kind = Some(crash_kind.as_str().to_string());
            let _ = write_receipt(&mut active.file, &active.data);
        }
    }

    pub fn complete(&self) {
        let active = self.slot.lock().ok().and_then(|mut slot| slot.take());
        let Some(mut active) = active else {
            return;
        };
        active.data.completed = true;
        let _ = write_receipt(&mut active.file, &active.data);
        drop_locked(active.file);
        let _ = std::fs::remove_file(active.path);
    }
}

impl RunReceiptRecovery {
    pub fn records(&self) -> impl Iterator<Item = MetricRecord> + '_ {
        self.receipts.iter().map(|receipt| receipt.record.clone())
    }

    pub fn acknowledge(mut self) {
        for mut receipt in self.receipts.drain(..) {
            if let Some(file) = receipt.file.take() {
                drop_locked(file);
            }
            let _ = std::fs::remove_file(receipt.path);
        }
        self.release_directory_lock();
    }

    pub(crate) fn defer_acknowledgement(mut self) -> DeferredRunReceiptAcknowledgement {
        let paths = self
            .receipts
            .drain(..)
            .map(|mut receipt| {
                if let Some(file) = receipt.file.take() {
                    drop_locked(file);
                }
                receipt.path
            })
            .collect();
        let directory = std::mem::take(&mut self.directory);
        self.release_directory_lock();
        DeferredRunReceiptAcknowledgement { directory, paths }
    }

    fn discard(&mut self) {
        for receipt in &mut self.receipts {
            if let Some(file) = receipt.file.take() {
                drop_locked(file);
            }
            let _ = std::fs::remove_file(&receipt.path);
        }
        self.receipts.clear();
        self.release_directory_lock();
    }

    fn release_directory_lock(&mut self) {
        if let Some(file) = self.directory_lock.take() {
            drop_locked(file);
        }
    }
}

impl DeferredRunReceiptAcknowledgement {
    pub(crate) fn acknowledge(self) {
        if self.paths.is_empty() {
            return;
        }
        let Ok(directory_lock) = open_or_create_private_file(&self.directory.join(REAPER_LOCK)) else {
            return;
        };
        if lock_file(&directory_lock, false).is_err() {
            return;
        }
        for path in self.paths {
            let file = match open_existing_private_file(&path) {
                Ok(file) => file,
                Err(err) if err.kind() == ErrorKind::NotFound => continue,
                Err(_) => continue,
            };
            if lock_file(&file, true).is_err() {
                continue;
            }
            drop_locked(file);
            let _ = std::fs::remove_file(path);
        }
        drop_locked(directory_lock);
    }
}

impl Drop for RunReceiptRecovery {
    fn drop(&mut self) {
        for receipt in &mut self.receipts {
            if let Some(file) = receipt.file.take() {
                drop_locked(file);
            }
        }
        self.release_directory_lock();
    }
}

fn register_panic_receipt(slot: &ReceiptSlot) {
    PANIC_RECEIPTS
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .expect("panic receipt registry mutex poisoned")
        .push(Arc::downgrade(slot));
    PANIC_HOOK.call_once(|| {
        let prior = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if let Some(receipts) = PANIC_RECEIPTS.get()
                && let Ok(mut receipts) = receipts.try_lock()
            {
                receipts.retain(|receipt| {
                    let Some(slot) = receipt.upgrade() else {
                        return false;
                    };
                    if let Ok(mut active) = slot.try_lock()
                        && let Some(active) = active.as_mut()
                    {
                        active.data.crash_kind = Some(metric::CrashKind::Panic.as_str().to_string());
                        let _ = write_receipt(&mut active.file, &active.data);
                    }
                    true
                });
            }
            prior(info);
        }));
    });
}

fn receipt_paths(directory: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "json"))
        .collect()
}

fn read_receipt(path: &Path) -> Option<ReceiptData> {
    let metadata = std::fs::metadata(path).ok()?;
    if metadata.len() == 0 || metadata.len() > MAX_RECEIPT_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn receipt_expired(data: &ReceiptData) -> bool {
    now_unix_millis().saturating_sub(data.started_unix_millis) > MAX_RECEIPT_AGE.as_millis() as u64
}

fn atomic_write(path: &Path, data: &ReceiptData) -> std::io::Result<()> {
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    let mut file = create_new_private_file(&temporary)?;
    write_receipt(&mut file, data)?;
    #[cfg(windows)]
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    std::fs::rename(temporary, path)
}

fn write_receipt(file: &mut File, data: &ReceiptData) -> std::io::Result<()> {
    let bytes = serde_json::to_vec(data)?;
    if bytes.len() as u64 > MAX_RECEIPT_BYTES {
        return Err(std::io::Error::new(
            ErrorKind::InvalidData,
            "run receipt exceeded its size limit",
        ));
    }
    file.seek(SeekFrom::Start(0))?;
    file.set_len(0)?;
    file.write_all(&bytes)?;
    file.sync_data()
}

fn create_new_private_file(path: &Path) -> std::io::Result<File> {
    private_file(path, true, false)
}

fn open_or_create_private_file(path: &Path) -> std::io::Result<File> {
    private_file(path, false, true)
}

fn open_existing_private_file(path: &Path) -> std::io::Result<File> {
    private_file(path, false, false)
}

fn private_file(path: &Path, create_new: bool, create: bool) -> std::io::Result<File> {
    if let Some(parent) = path.parent()
        && (create_new || create)
    {
        std::fs::create_dir_all(parent)?;
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true);
    if create_new {
        options.create_new(true);
    } else if create {
        options.create(true);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

#[cfg(unix)]
fn lock_file(file: &File, nonblocking: bool) -> std::io::Result<()> {
    use std::os::fd::AsRawFd;
    let operation = libc::LOCK_EX | if nonblocking { libc::LOCK_NB } else { 0 };
    if unsafe { libc::flock(file.as_raw_fd(), operation) } == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(unix)]
fn unlock_file(file: &File) -> std::io::Result<()> {
    use std::os::fd::AsRawFd;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_UN) } == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(windows)]
fn lock_file(file: &File, nonblocking: bool) -> std::io::Result<()> {
    use std::os::windows::io::AsRawHandle;

    use windows_sys::Win32::Storage::FileSystem::{
        LOCKFILE_EXCLUSIVE_LOCK,
        LOCKFILE_FAIL_IMMEDIATELY,
        LockFileEx,
    };
    use windows_sys::Win32::System::IO::OVERLAPPED;

    let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
    let flags = LOCKFILE_EXCLUSIVE_LOCK | if nonblocking { LOCKFILE_FAIL_IMMEDIATELY } else { 0 };
    let success = unsafe { LockFileEx(file.as_raw_handle() as _, flags, 0, u32::MAX, u32::MAX, &mut overlapped) };
    if success != 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(windows)]
fn unlock_file(file: &File) -> std::io::Result<()> {
    use std::os::windows::io::AsRawHandle;

    use windows_sys::Win32::Storage::FileSystem::UnlockFileEx;
    use windows_sys::Win32::System::IO::OVERLAPPED;

    let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
    let success = unsafe { UnlockFileEx(file.as_raw_handle() as _, 0, u32::MAX, u32::MAX, &mut overlapped) };
    if success != 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

fn drop_locked(file: File) {
    let _ = unlock_file(&file);
    drop(file);
}

fn current_os_type() -> metric::OsType {
    metric::OsType::from_name(std::env::consts::OS)
}

fn now_unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> ProcessIdentity {
        ProcessIdentity::new(metric::Engine::V2, metric::ProcessRole::Host)
    }

    fn receipt_data() -> ReceiptData {
        ReceiptData {
            schema_version: 1,
            run_id: Uuid::new_v4(),
            pid: std::process::id(),
            started_unix_millis: now_unix_millis(),
            version_full: env!("CARGO_PKG_VERSION").to_string(),
            engine: metric::Engine::V2.as_str().to_string(),
            os_type: metric::OsType::Macos.as_str().to_string(),
            process_role: metric::ProcessRole::Host.as_str().to_string(),
            crash_kind: None,
            completed: false,
        }
    }

    fn write_test_receipt(store: &RunReceiptStore, data: &ReceiptData) -> PathBuf {
        let path = store.directory.join(format!("{}.json", data.run_id));
        std::fs::create_dir_all(&store.directory).unwrap();
        atomic_write(&path, data).unwrap();
        path
    }

    fn receipt_directory_bytes(store: &RunReceiptStore) -> u64 {
        receipt_paths(&store.directory)
            .iter()
            .map(|path| std::fs::metadata(path).unwrap().len())
            .sum()
    }

    #[test]
    fn clean_completion_removes_the_receipt() {
        let directory = tempfile::tempdir().unwrap();
        let store = RunReceiptStore::new(directory.path());
        let receipt = store.start(identity()).unwrap();
        assert_eq!(receipt_paths(&store.directory).len(), 1);

        receipt.complete();

        assert!(receipt_paths(&store.directory).is_empty());
    }

    #[test]
    fn recovers_an_unlocked_incomplete_receipt_with_original_dimensions() {
        let directory = tempfile::tempdir().unwrap();
        let store = RunReceiptStore::new(directory.path());
        let receipt = store.start(identity()).unwrap();
        receipt.mark_crash(metric::CrashKind::Panic);
        drop(receipt);

        let recovery = store.recover();
        let records = recovery.records().collect::<Vec<_>>();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].name, "kiro_cli_crash_total");
        assert!(
            records[0]
                .attributes
                .iter()
                .any(|attribute| attribute.key == "crash_kind" && attribute.value == "panic")
        );
        recovery.acknowledge();
        assert!(receipt_paths(&store.directory).is_empty());
    }

    #[test]
    fn does_not_recover_a_locked_active_receipt() {
        let directory = tempfile::tempdir().unwrap();
        let store = RunReceiptStore::new(directory.path());
        let receipt = store.start(identity()).unwrap();

        assert_eq!(store.recover().records().count(), 0);

        receipt.complete();
    }

    #[test]
    fn failed_recovery_keeps_the_receipt_for_a_later_launch() {
        let directory = tempfile::tempdir().unwrap();
        let store = RunReceiptStore::new(directory.path());
        drop(store.start(identity()).unwrap());

        drop(store.recover());

        assert_eq!(store.recover().records().count(), 1);
    }

    #[test]
    fn recovery_deletes_corrupt_receipts() {
        let directory = tempfile::tempdir().unwrap();
        let store = RunReceiptStore::new(directory.path());
        std::fs::create_dir_all(&store.directory).unwrap();
        let path = store.directory.join(format!("{}.json", Uuid::new_v4()));
        std::fs::write(&path, b"{not-json").unwrap();

        assert_eq!(store.recover().records().count(), 0);

        assert!(!path.exists());
    }

    #[test]
    fn recovery_deletes_oversized_receipts() {
        let directory = tempfile::tempdir().unwrap();
        let store = RunReceiptStore::new(directory.path());
        std::fs::create_dir_all(&store.directory).unwrap();
        let path = store.directory.join(format!("{}.json", Uuid::new_v4()));
        std::fs::write(&path, vec![b'x'; MAX_RECEIPT_BYTES as usize + 1]).unwrap();

        assert_eq!(store.recover().records().count(), 0);

        assert!(!path.exists());
    }

    #[test]
    fn recovery_deletes_expired_receipts() {
        let directory = tempfile::tempdir().unwrap();
        let store = RunReceiptStore::new(directory.path());
        let mut data = receipt_data();
        data.started_unix_millis = now_unix_millis().saturating_sub(MAX_RECEIPT_AGE.as_millis() as u64 + 1);
        let path = write_test_receipt(&store, &data);

        assert_eq!(store.recover().records().count(), 0);

        assert!(!path.exists());
    }

    #[test]
    fn start_prunes_receipts_to_max_receipts() {
        let directory = tempfile::tempdir().unwrap();
        let store = RunReceiptStore::new(directory.path());
        for _ in 0..MAX_RECEIPTS {
            write_test_receipt(&store, &receipt_data());
        }

        let current = store.start(identity()).unwrap();

        assert_eq!(receipt_paths(&store.directory).len(), MAX_RECEIPTS);
        current.complete();
    }

    #[test]
    fn pruning_enforces_max_directory_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let store = RunReceiptStore::new(directory.path());
        while receipt_directory_bytes(&store) <= MAX_DIRECTORY_BYTES {
            let mut data = receipt_data();
            data.version_full = "v".repeat(700);
            write_test_receipt(&store, &data);
        }
        assert!(receipt_directory_bytes(&store) > MAX_DIRECTORY_BYTES);

        store.prune_to_limits(usize::MAX, MAX_DIRECTORY_BYTES).unwrap();

        assert!(receipt_directory_bytes(&store) <= MAX_DIRECTORY_BYTES);
    }

    #[test]
    fn failed_post_write_start_removes_incomplete_receipt() {
        let directory = tempfile::tempdir().unwrap();
        let store = RunReceiptStore::new(directory.path());

        let result = store.start_with_opener(identity(), |_| Err(std::io::Error::other("injected open failure")));

        assert_eq!(result.unwrap_err().kind(), ErrorKind::Other);
        assert!(receipt_paths(&store.directory).is_empty());
    }

    #[test]
    fn deferred_acknowledgement_releases_locks_and_removes_only_recovered_receipts() {
        let directory = tempfile::tempdir().unwrap();
        let store = RunReceiptStore::new(directory.path());
        drop(store.start(identity()).unwrap());

        let recovery = store.recover();
        assert_eq!(recovery.records().count(), 1);
        let acknowledgement = recovery.defer_acknowledgement();
        let current = store.start(identity()).unwrap();

        acknowledgement.acknowledge();

        assert_eq!(receipt_paths(&store.directory).len(), 1);
        current.complete();
    }

    #[test]
    fn deferred_acknowledgement_skips_receipt_deleted_before_open() {
        let directory = tempfile::tempdir().unwrap();
        let store = RunReceiptStore::new(directory.path());
        drop(store.start(identity()).unwrap());

        let recovery = store.recover();
        let path = recovery.receipts[0].path.clone();
        let acknowledgement = recovery.defer_acknowledgement();
        std::fs::remove_file(&path).unwrap();

        acknowledgement.acknowledge();

        assert!(matches!(
            open_existing_private_file(&path),
            Err(err) if err.kind() == ErrorKind::NotFound
        ));
        assert!(receipt_paths(&store.directory).is_empty());
    }
}
