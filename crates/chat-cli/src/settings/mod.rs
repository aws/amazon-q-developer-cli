pub mod error;
#[allow(clippy::module_inception)]
pub mod settings;
pub mod sqlite;
pub mod state;

use std::fs::{
    self,
    File,
};
use std::io::{
    Seek,
    SeekFrom,
    Write,
};
use std::path::PathBuf;

pub use error::{
    Result,
    SettingsError,
};
use fd_lock::RwLock as FileRwLock;
use parking_lot::{
    MappedRwLockReadGuard,
    MappedRwLockWriteGuard,
    RwLock,
    RwLockReadGuard,
    RwLockWriteGuard,
};
use serde_json::Value;
pub use settings::Settings;
pub use state::State;

use crate::fig_util::directories;

pub type Map = serde_json::Map<String, Value>;

static SETTINGS_FILE_LOCK: RwLock<()> = RwLock::new(());

static SETTINGS_DATA: RwLock<Option<Map>> = RwLock::new(None);

#[derive(Debug, Clone)]
pub enum Backend {
    Global,
    Memory(Map),
}

pub enum ReadGuard<'a, T> {
    Global(RwLockReadGuard<'a, Option<T>>),
    Memory(&'a T),
}

impl<'a, T> ReadGuard<'a, T> {
    pub fn try_map<U, F: FnOnce(&T) -> Option<&U>>(self, f: F) -> Option<MappedReadGuard<'a, U>> {
        match self {
            ReadGuard::Global(guard) => RwLockReadGuard::<'a, Option<T>>::try_map(guard, |data: &Option<T>| {
                f(data.as_ref().expect("global backend is not used"))
            })
            .ok()
            .map(MappedReadGuard::Global),
            ReadGuard::Memory(data) => f(data).map(MappedReadGuard::Memory),
        }
    }
}

impl<T> std::ops::Deref for ReadGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match self {
            ReadGuard::Global(guard) => guard.as_ref().expect("global backend is not used"),
            ReadGuard::Memory(data) => data,
        }
    }
}

pub enum MappedReadGuard<'a, T> {
    Global(MappedRwLockReadGuard<'a, T>),
    Memory(&'a T),
}

impl<T> std::ops::Deref for MappedReadGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match self {
            MappedReadGuard::Global(guard) => guard,
            MappedReadGuard::Memory(data) => data,
        }
    }
}

pub enum WriteGuard<'a, T> {
    Global(RwLockWriteGuard<'a, Option<T>>),
    Memory(&'a mut T),
}

impl<'a, T> WriteGuard<'a, T> {
    pub fn try_map<U, F: FnOnce(&mut T) -> Option<&mut U>>(self, f: F) -> Option<MappedWriteGuard<'a, U>> {
        match self {
            WriteGuard::Global(guard) => RwLockWriteGuard::<'a, Option<T>>::try_map(guard, |data: &mut Option<T>| {
                f(data.as_mut().expect("global backend is not used"))
            })
            .ok()
            .map(MappedWriteGuard::Global),
            WriteGuard::Memory(data) => f(data).map(MappedWriteGuard::Memory),
        }
    }
}

impl<T> std::ops::Deref for WriteGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match self {
            WriteGuard::Global(guard) => guard.as_ref().expect("global backend is not used"),
            WriteGuard::Memory(data) => data,
        }
    }
}

impl<T> std::ops::DerefMut for WriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            WriteGuard::Global(guard) => guard.as_mut().expect("global backend is not used"),
            WriteGuard::Memory(data) => data,
        }
    }
}

pub enum MappedWriteGuard<'a, T> {
    Global(MappedRwLockWriteGuard<'a, T>),
    Memory(&'a mut T),
}

impl<T> std::ops::Deref for MappedWriteGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match self {
            MappedWriteGuard::Global(guard) => guard,
            MappedWriteGuard::Memory(data) => data,
        }
    }
}

impl<T> std::ops::DerefMut for MappedWriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            MappedWriteGuard::Global(guard) => guard,
            MappedWriteGuard::Memory(data) => data,
        }
    }
}

pub trait JsonStore: Sized {
    /// Path to the file
    fn path() -> Result<PathBuf>;

    /// In mem lock on the file
    fn file_lock() -> &'static RwLock<()>;

    /// [RwLock] on the data, [None] if not using the global backend
    fn data_lock() -> &'static RwLock<Option<Map>>;

    fn new_from_backend(backend: Backend) -> Self;

    fn map(&self) -> ReadGuard<'_, Map>;

    fn map_mut(&mut self) -> WriteGuard<'_, Map>;

    fn load() -> Result<Self> {
        let is_global = Self::data_lock().read().as_ref().is_some();
        if is_global {
            Ok(Self::new_from_backend(Backend::Global))
        } else {
            Ok(Self::new_from_backend(Backend::Memory(Self::load_from_file()?)))
        }
    }

    fn load_from_file() -> Result<Map> {
        let path = Self::path()?;

        // If the folder doesn't exist, create it.
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }

        let json: Map = {
            let _lock_guard = Self::file_lock().write();

            // If the file doesn't exist, create it.
            if !path.exists() {
                let mut file = FileRwLock::new(File::create(path)?);
                file.write()?.write_all(b"{}")?;
                serde_json::Map::new()
            } else {
                let mut file = FileRwLock::new(File::open(&path)?);
                let mut read = file.write()?;
                serde_json::from_reader(&mut *read)?
            }
        };

        Ok(json)
    }

    fn save_to_file(&self) -> Result<()> {
        let path = Self::path()?;

        // If the folder doesn't exist, create it.
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }

        let _lock_guard = Self::file_lock().write();

        let mut file_opts = File::options();
        file_opts.create(true).write(true).truncate(true);

        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            file_opts.mode(0o600);
        }

        let mut file = FileRwLock::new(file_opts.open(&path)?);
        let mut lock = file.write()?;

        if let Err(_err) = serde_json::to_writer_pretty(&mut *lock, &*self.map()) {
            // Write {} to the file if the serialization failed
            lock.seek(SeekFrom::Start(0))?;
            lock.set_len(0)?;
            lock.write_all(b"{}")?;
        };
        lock.flush()?;

        Ok(())
    }

    fn set(&mut self, key: impl Into<String>, value: impl Into<serde_json::Value>) {
        self.map_mut().insert(key.into(), value.into());
    }

    fn get(&self, key: impl AsRef<str>) -> Option<MappedReadGuard<'_, Value>> {
        self.map().try_map(|data| data.get(key.as_ref()))
    }

    fn remove(&mut self, key: impl AsRef<str>) -> Option<Value> {
        self.map_mut().remove(key.as_ref())
    }

    #[allow(dead_code)]
    fn get_mut(&mut self, key: impl Into<String>) -> Option<MappedWriteGuard<'_, Value>> {
        self.map_mut().try_map(|data| data.get_mut(&key.into()))
    }

    fn get_bool(&self, key: impl AsRef<str>) -> Option<bool> {
        self.get(key).and_then(|value| value.as_bool())
    }

    #[allow(dead_code)]
    fn get_bool_or(&self, key: impl AsRef<str>, default: bool) -> bool {
        self.get_bool(key).unwrap_or(default)
    }

    fn get_string(&self, key: impl AsRef<str>) -> Option<String> {
        self.get(key).and_then(|value| value.as_str().map(|s| s.into()))
    }

    #[allow(dead_code)]
    fn get_string_or(&self, key: impl AsRef<str>, default: String) -> String {
        self.get_string(key).unwrap_or(default)
    }

    fn get_int(&self, key: impl AsRef<str>) -> Option<i64> {
        self.get(key).and_then(|value| value.as_i64())
    }

    #[allow(dead_code)]
    fn get_int_or(&self, key: impl AsRef<str>, default: i64) -> i64 {
        self.get_int(key).unwrap_or(default)
    }
}

pub struct OldSettings {
    pub(crate) inner: Backend,
}

impl JsonStore for OldSettings {
    fn path() -> Result<PathBuf> {
        Ok(directories::settings_path()?)
    }

    fn file_lock() -> &'static RwLock<()> {
        &SETTINGS_FILE_LOCK
    }

    fn data_lock() -> &'static RwLock<Option<Map>> {
        &SETTINGS_DATA
    }

    fn new_from_backend(backend: Backend) -> Self {
        match backend {
            Backend::Global => Self { inner: Backend::Global },
            Backend::Memory(map) => Self {
                inner: Backend::Memory(map),
            },
        }
    }

    fn map(&self) -> ReadGuard<'_, Map> {
        match &self.inner {
            Backend::Global => ReadGuard::Global(Self::data_lock().read()),
            Backend::Memory(map) => ReadGuard::Memory(map),
        }
    }

    fn map_mut(&mut self) -> WriteGuard<'_, Map> {
        match &mut self.inner {
            Backend::Global => WriteGuard::Global(Self::data_lock().write()),
            Backend::Memory(map) => WriteGuard::Memory(map),
        }
    }
}
