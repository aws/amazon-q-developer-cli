use std::collections::BTreeMap;
use std::fs::{
    File,
    OpenOptions,
};
use std::io::{
    ErrorKind,
    Read,
    Write,
};
use std::path::{
    Path,
    PathBuf,
};

use serde::{
    Deserialize,
    Serialize,
};

const STORE_FILE: &str = "telemetry-export-drops.json";
const LOCK_FILE: &str = "telemetry-export-drops.lock";
const MAX_STORE_BYTES: u64 = 16 * 1024;
const MAX_ENTRIES: usize = 32;

#[derive(Clone, Debug)]
pub(crate) struct ExportDropStore {
    state_dir: PathBuf,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub(crate) struct ExportDropKey {
    pub version_full: String,
    pub signal: String,
    pub drop_reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct ExportDropAggregate {
    #[serde(flatten)]
    pub key: ExportDropKey,
    pub count: u64,
}

impl ExportDropStore {
    pub(crate) fn new(state_dir: PathBuf) -> Self {
        Self { state_dir }
    }

    pub(crate) fn record(&self, signal: &str, drop_reason: &str, count: u64) {
        if count == 0 {
            return;
        }
        let key = ExportDropKey {
            version_full: env!("CARGO_PKG_VERSION").to_string(),
            signal: signal.to_string(),
            drop_reason: drop_reason.to_string(),
        };
        let _ = self.with_locked(|path| {
            let mut values = read_values(path);
            let value = values.entry(key).or_default();
            *value = value.saturating_add(count);
            trim_values(&mut values);
            write_values(path, &values)
        });
    }

    pub(crate) fn snapshot(&self) -> Vec<ExportDropAggregate> {
        self.with_locked(|path| {
            Ok(read_values(path)
                .into_iter()
                .map(|(key, count)| ExportDropAggregate { key, count })
                .collect())
        })
        .unwrap_or_default()
    }

    pub(crate) fn subtract(&self, snapshot: &[ExportDropAggregate]) {
        if snapshot.is_empty() {
            return;
        }
        let _ = self.with_locked(|path| {
            let mut values = read_values(path);
            for entry in snapshot {
                let Some(value) = values.get_mut(&entry.key) else {
                    continue;
                };
                *value = value.saturating_sub(entry.count);
                if *value == 0 {
                    values.remove(&entry.key);
                }
            }
            write_values(path, &values)
        });
    }

    pub(crate) fn clear(&self) {
        let _ = self.with_locked(|path| match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err),
        });
    }

    fn with_locked<T>(&self, work: impl FnOnce(&Path) -> std::io::Result<T>) -> std::io::Result<T> {
        std::fs::create_dir_all(&self.state_dir)?;
        let lock_file = private_file(&self.state_dir.join(LOCK_FILE))?;
        let mut lock = fd_lock::RwLock::new(lock_file);
        let _guard = lock.write()?;
        work(&self.state_dir.join(STORE_FILE))
    }
}

fn private_file(path: &Path) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

fn read_values(path: &Path) -> BTreeMap<ExportDropKey, u64> {
    let Ok(file) = File::open(path) else {
        return BTreeMap::new();
    };
    if file.metadata().is_ok_and(|metadata| metadata.len() > MAX_STORE_BYTES) {
        return BTreeMap::new();
    }
    let mut bytes = Vec::new();
    if file.take(MAX_STORE_BYTES + 1).read_to_end(&mut bytes).is_err() || bytes.len() as u64 > MAX_STORE_BYTES {
        return BTreeMap::new();
    }
    serde_json::from_slice::<Vec<ExportDropAggregate>>(&bytes)
        .unwrap_or_default()
        .into_iter()
        .filter(|entry| entry.count > 0)
        .map(|entry| (entry.key, entry.count))
        .collect()
}

fn trim_values(values: &mut BTreeMap<ExportDropKey, u64>) {
    while values.len() > MAX_ENTRIES {
        let Some(key) = values.keys().next().cloned() else {
            break;
        };
        values.remove(&key);
    }
}

fn write_values(path: &Path, values: &BTreeMap<ExportDropKey, u64>) -> std::io::Result<()> {
    if values.is_empty() {
        return match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err),
        };
    }

    let entries = values
        .iter()
        .map(|(key, count)| ExportDropAggregate {
            key: key.clone(),
            count: *count,
        })
        .collect::<Vec<_>>();
    let bytes = serde_json::to_vec(&entries)?;
    if bytes.len() as u64 > MAX_STORE_BYTES {
        return Err(std::io::Error::new(
            ErrorKind::InvalidData,
            "telemetry export-drop store exceeded its size limit",
        ));
    }

    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    let mut file = private_file(&temporary)?;
    file.set_len(0)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    #[cfg(windows)]
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    std::fs::rename(temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_new_drops_when_a_replayed_snapshot_is_removed() {
        let dir = tempfile::tempdir().unwrap();
        let store = ExportDropStore::new(dir.path().to_path_buf());
        store.record("metrics", "oversize", 2);
        let snapshot = store.snapshot();
        store.record("metrics", "oversize", 3);

        store.subtract(&snapshot);

        let remaining = store.snapshot();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].count, 3);
    }

    #[test]
    fn corrupt_or_oversize_state_fails_closed_to_an_empty_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let store = ExportDropStore::new(dir.path().to_path_buf());
        std::fs::write(dir.path().join(STORE_FILE), b"not json").unwrap();
        assert!(store.snapshot().is_empty());

        std::fs::write(dir.path().join(STORE_FILE), vec![b'x'; MAX_STORE_BYTES as usize + 1]).unwrap();
        assert!(store.snapshot().is_empty());
    }

    #[test]
    fn clear_removes_persisted_counts() {
        let dir = tempfile::tempdir().unwrap();
        let store = ExportDropStore::new(dir.path().to_path_buf());
        store.record("metrics", "retry_exhausted", 1);

        store.clear();

        assert!(store.snapshot().is_empty());
    }
}
