use serde::de::DeserializeOwned;
use serde_json::{
    Map,
    Value,
};

use super::sqlite::{
    Db,
    database,
};
use crate::settings::Result;

#[derive(Debug, Clone, Default)]
pub struct State(inner::Inner);

mod inner {
    use super::*;

    #[derive(Debug, Clone, Default)]
    pub enum Inner {
        #[default]
        Real,
        Fake(Db),
    }
}

impl State {
    pub fn new() -> Self {
        if cfg!(test) {
            let db = Db::mock();
            db.migrate().unwrap();
            return Self(inner::Inner::Fake(db));
        }

        Self::default()
    }

    fn database(&self) -> Result<&Db> {
        match &self.0 {
            inner::Inner::Real => Ok(database()?),
            inner::Inner::Fake(db) => Ok(db),
        }
    }

    pub fn all(&self) -> Result<Map<String, Value>> {
        self.database()?.all_state_values()
    }

    pub fn set_value(&self, key: impl AsRef<str>, value: impl Into<Value>) -> Result<()> {
        self.database()?.set_state_value(key, value)?;
        Ok(())
    }

    pub fn remove_value(&self, key: impl AsRef<str>) -> Result<()> {
        self.database()?.unset_state_value(key)?;
        Ok(())
    }

    pub fn get_value(&self, key: impl AsRef<str>) -> Result<Option<Value>> {
        self.database()?.get_state_value(key)
    }

    pub fn get<T: DeserializeOwned>(&self, key: impl AsRef<str>) -> Result<Option<T>> {
        Ok(self
            .database()?
            .get_state_value(key)?
            .map(|value| serde_json::from_value(value.clone()))
            .transpose()?)
    }

    pub fn get_bool(&self, key: impl AsRef<str>) -> Result<Option<bool>> {
        Ok(self.database()?.get_state_value(key)?.and_then(|value| value.as_bool()))
    }

    pub fn get_bool_or(&self, key: impl AsRef<str>, default: bool) -> bool {
        self.get_bool(key).ok().flatten().unwrap_or(default)
    }

    pub fn get_string(&self, key: impl AsRef<str>) -> Result<Option<String>> {
        Ok(self.database()?.get_state_value(key)?.and_then(|value| match value {
            Value::String(s) => Some(s),
            _ => None,
        }))
    }

    pub fn get_string_or(&self, key: impl AsRef<str>, default: impl Into<String>) -> String {
        self.get_string(key).ok().flatten().unwrap_or_else(|| default.into())
    }

    pub fn get_int(&self, key: impl AsRef<str>) -> Result<Option<i64>> {
        Ok(self.database()?.get_state_value(key)?.and_then(|value| value.as_i64()))
    }

    pub fn get_int_or(&self, key: impl AsRef<str>, default: i64) -> i64 {
        self.get_int(key).ok().flatten().unwrap_or(default)
    }

    // Atomic style operations

    pub fn atomic_bool_or(&self, key: impl AsRef<str>, or: bool) -> Result<bool> {
        self.database()?.atomic_bool_or(key, or)
    }
}

#[allow(dead_code)]
pub fn all() -> Result<Map<String, Value>> {
    State::new().all()
}

pub fn set_value(key: impl AsRef<str>, value: impl Into<Value>) -> Result<()> {
    State::new().set_value(key, value)
}

pub fn remove_value(key: impl AsRef<str>) -> Result<()> {
    State::new().remove_value(key)
}

pub fn get_value(key: impl AsRef<str>) -> Result<Option<Value>> {
    State::new().get_value(key)
}

pub fn get<T: DeserializeOwned>(key: impl AsRef<str>) -> Result<Option<T>> {
    State::new().get(key)
}

#[allow(dead_code)]
pub fn get_bool(key: impl AsRef<str>) -> Result<Option<bool>> {
    State::new().get_bool(key)
}

#[allow(dead_code)]
pub fn get_bool_or(key: impl AsRef<str>, default: bool) -> bool {
    State::new().get_bool_or(key, default)
}

pub fn get_string(key: impl AsRef<str>) -> Result<Option<String>> {
    State::new().get_string(key)
}

#[allow(dead_code)]
pub fn get_string_or(key: impl AsRef<str>, default: impl Into<String>) -> String {
    State::new().get_string_or(key, default)
}

#[allow(dead_code)]
pub fn get_int(key: impl AsRef<str>) -> Result<Option<i64>> {
    State::new().get_int(key)
}

#[allow(dead_code)]
pub fn get_int_or(key: impl AsRef<str>, default: i64) -> i64 {
    State::new().get_int_or(key, default)
}

#[cfg(test)]
mod tests {
    use super::{
        Result,
        State,
    };

    /// General read/write state test
    #[test]
    fn test_state() -> Result<()> {
        let state = State::new();

        assert!(state.get_value("test").unwrap().is_none());
        assert!(state.get::<String>("test").unwrap().is_none());
        state.set_value("test", "hello :)")?;
        assert!(state.get_value("test").unwrap().is_some());
        assert!(state.get::<String>("test").unwrap().is_some());
        state.remove_value("test")?;
        assert!(state.get_value("test").unwrap().is_none());
        assert!(state.get::<String>("test").unwrap().is_none());

        assert!(!state.get_bool_or("bool", false));
        state.set_value("bool", true).unwrap();
        assert!(state.get_bool("bool").unwrap().unwrap());

        assert_eq!(state.get_string_or("string", "hi"), "hi");
        state.set_value("string", "hi").unwrap();
        assert_eq!(state.get_string("string").unwrap().unwrap(), "hi");

        assert_eq!(state.get_int_or("int", 32), 32);
        state.set_value("int", 32).unwrap();
        assert_eq!(state.get_int("int").unwrap().unwrap(), 32);

        Ok(())
    }
}
