use std::sync::{
    Arc,
    Mutex,
};

use serde::de::DeserializeOwned;
use serde_json::Map;

use super::{
    JsonStore,
    OldSettings,
    Result,
};

#[derive(Debug, Clone, Default)]
pub struct Settings(inner::Inner);

mod inner {
    use std::sync::{
        Arc,
        Mutex,
    };

    use serde_json::{
        Map,
        Value,
    };

    #[derive(Debug, Clone, Default)]
    pub enum Inner {
        #[default]
        Real,
        Fake(Arc<Mutex<Map<String, Value>>>),
    }
}

impl Settings {
    pub fn new() -> Self {
        match cfg!(test) {
            true => Self(inner::Inner::Fake(Arc::new(Mutex::new(Map::new())))),
            false => Self(inner::Inner::Real),
        }
    }

    pub fn set_value(&self, key: impl Into<String>, value: impl Into<serde_json::Value>) -> Result<()> {
        match &self.0 {
            inner::Inner::Real => {
                let mut settings = OldSettings::load()?;
                settings.set(key, value);
                settings.save_to_file()?;
                Ok(())
            },
            inner::Inner::Fake(map) => {
                map.lock()?.insert(key.into(), value.into());
                Ok(())
            },
        }
    }

    pub fn remove_value(&self, key: impl AsRef<str>) -> Result<()> {
        match &self.0 {
            inner::Inner::Real => {
                let mut settings = OldSettings::load()?;
                settings.remove(key);
                settings.save_to_file()?;
                Ok(())
            },
            inner::Inner::Fake(map) => {
                map.lock()?.remove(key.as_ref());
                Ok(())
            },
        }
    }

    pub fn get_value(&self, key: impl AsRef<str>) -> Result<Option<serde_json::Value>> {
        match &self.0 {
            inner::Inner::Real => Ok(OldSettings::load()?.get(key.as_ref()).map(|v| v.clone())),
            inner::Inner::Fake(map) => Ok(map.lock()?.get(key.as_ref()).cloned()),
        }
    }

    #[allow(dead_code)]
    pub fn get<T: DeserializeOwned>(&self, key: impl AsRef<str>) -> Result<Option<T>> {
        match &self.0 {
            inner::Inner::Real => {
                let settings = OldSettings::load()?;
                let v = settings.get(key);
                match v.as_deref() {
                    Some(value) => Ok(Some(serde_json::from_value(value.clone())?)),
                    None => Ok(None),
                }
            },
            inner::Inner::Fake(map) => {
                let value = map.lock()?.get(key.as_ref()).cloned();
                match value {
                    Some(value) => Ok(Some(serde_json::from_value(value)?)),
                    None => Ok(None),
                }
            },
        }
    }

    pub fn get_bool(&self, key: impl AsRef<str>) -> Result<Option<bool>> {
        match &self.0 {
            inner::Inner::Real => Ok(OldSettings::load()?.get_bool(key.as_ref())),
            inner::Inner::Fake(map) => Ok(map.lock()?.get(key.as_ref()).cloned().and_then(|v| v.as_bool())),
        }
    }

    pub fn get_bool_or(&self, key: impl AsRef<str>, default: bool) -> bool {
        self.get_bool(key).ok().flatten().unwrap_or(default)
    }

    pub fn get_string(&self, key: impl AsRef<str>) -> Result<Option<String>> {
        match &self.0 {
            inner::Inner::Real => Ok(OldSettings::load()?.get_string(key.as_ref())),
            inner::Inner::Fake(map) => Ok(map
                .lock()?
                .get(key.as_ref())
                .cloned()
                .and_then(|v| v.as_str().map(|s| s.to_owned()))),
        }
    }

    pub fn get_string_opt(&self, key: impl AsRef<str>) -> Option<String> {
        self.get_string(key).ok().flatten()
    }

    #[allow(dead_code)]
    pub fn get_string_or(&self, key: impl AsRef<str>, default: String) -> String {
        self.get_string(key).ok().flatten().unwrap_or(default)
    }

    pub fn get_int(&self, key: impl AsRef<str>) -> Result<Option<i64>> {
        match &self.0 {
            inner::Inner::Real => Ok(OldSettings::load()?.get_int(key.as_ref())),
            inner::Inner::Fake(map) => Ok(map.lock()?.get(key.as_ref()).cloned().and_then(|v| v.as_i64())),
        }
    }

    #[allow(dead_code)]
    pub fn get_int_or(&self, key: impl AsRef<str>, default: i64) -> i64 {
        self.get_int(key).ok().flatten().unwrap_or(default)
    }
}

pub fn set_value(key: impl Into<String>, value: impl Into<serde_json::Value>) -> Result<()> {
    Settings::new().set_value(key, value)
}

pub fn remove_value(key: impl AsRef<str>) -> Result<()> {
    Settings::new().remove_value(key)
}

pub fn get_value(key: impl AsRef<str>) -> Result<Option<serde_json::Value>> {
    Settings::new().get_value(key)
}

#[allow(dead_code)]
pub fn get<T: DeserializeOwned>(key: impl AsRef<str>) -> Result<Option<T>> {
    Settings::new().get(key)
}

#[allow(dead_code)]
pub fn get_bool(key: impl AsRef<str>) -> Result<Option<bool>> {
    Settings::new().get_bool(key)
}

pub fn get_bool_or(key: impl AsRef<str>, default: bool) -> bool {
    Settings::new().get_bool_or(key, default)
}

#[allow(dead_code)]
pub fn get_string(key: impl AsRef<str>) -> Result<Option<String>> {
    Settings::new().get_string(key)
}

pub fn get_string_opt(key: impl AsRef<str>) -> Option<String> {
    Settings::new().get_string_opt(key)
}

#[allow(dead_code)]
pub fn get_string_or(key: impl AsRef<str>, default: String) -> String {
    Settings::new().get_string_or(key, default)
}

pub fn get_int(key: impl AsRef<str>) -> Result<Option<i64>> {
    Settings::new().get_int(key)
}

#[allow(dead_code)]
pub fn get_int_or(key: impl AsRef<str>, default: i64) -> i64 {
    Settings::new().get_int_or(key, default)
}

#[cfg(test)]
mod test {
    use super::{
        Result,
        Settings,
    };

    /// General read/write settings test
    #[test]
    fn test_settings() -> Result<()> {
        let settings = Settings::new();

        assert!(settings.get_value("test").unwrap().is_none());
        assert!(settings.get::<String>("test").unwrap().is_none());
        settings.set_value("test", "hello :)")?;
        assert!(settings.get_value("test").unwrap().is_some());
        assert!(settings.get::<String>("test").unwrap().is_some());
        settings.remove_value("test")?;
        assert!(settings.get_value("test").unwrap().is_none());
        assert!(settings.get::<String>("test").unwrap().is_none());

        assert!(!settings.get_bool_or("bool", false));
        settings.set_value("bool", true).unwrap();
        assert!(settings.get_bool("bool").unwrap().unwrap());

        assert_eq!(settings.get_string_or("string", "hi".into()), "hi");
        settings.set_value("string", "hi").unwrap();
        assert_eq!(settings.get_string("string").unwrap().unwrap(), "hi");

        assert_eq!(settings.get_int_or("int", 32), 32);
        settings.set_value("int", 32).unwrap();
        assert_eq!(settings.get_int("int").unwrap().unwrap(), 32);

        Ok(())
    }
}
