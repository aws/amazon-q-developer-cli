//! Fig.js Protocol Buffers

use serde::Serialize;
use serde::ser::{
    SerializeStruct,
    SerializeTuple,
};
use serde_json::Value;

use crate::proto::fig_common::*;

// Duration conversion

impl From<std::time::Duration> for Duration {
    fn from(value: std::time::Duration) -> Self {
        Self {
            secs: value.as_secs(),
            nanos: value.subsec_nanos(),
        }
    }
}

impl From<Duration> for std::time::Duration {
    fn from(value: Duration) -> Self {
        std::time::Duration::new(value.secs, value.nanos)
    }
}

// Context utils

impl ShellContext {}

impl Serialize for ShellContext {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut s = serializer.serialize_struct("ShellContext", 14)?;
        s.serialize_field("pid", &self.pid)?;
        s.serialize_field("ttys", &self.ttys)?;
        s.serialize_field("process_name", &self.process_name)?;
        s.serialize_field("current_working_directory", &self.current_working_directory)?;
        s.serialize_field("session_id", &self.session_id)?;
        s.serialize_field("terminal", &self.terminal)?;
        s.serialize_field("hostname", &self.hostname)?;
        s.serialize_field("shell_path", &self.shell_path)?;
        s.serialize_field("wsl_distro", &self.wsl_distro)?;
        s.serialize_field("environment_variables", &self.environment_variables)?;
        s.serialize_field("qterm_version", &self.qterm_version)?;
        s.serialize_field("preexec", &self.preexec)?;
        s.serialize_field("osc_lock", &self.osc_lock)?;
        s.serialize_field("alias", &self.alias)?;
        s.end()
    }
}

impl Serialize for EnvironmentVariable {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut s = serializer.serialize_tuple(2)?;
        s.serialize_element(&self.key)?;
        s.serialize_element(&self.value)?;
        s.end()
    }
}

// JSON utilities

impl From<String> for Json {
    fn from(s: String) -> Self {
        Self {
            value: Some(json::Value::String(s)),
        }
    }
}

impl From<u64> for Json {
    fn from(n: u64) -> Self {
        Self {
            value: Some(json::Value::Number(json::Number {
                number: Some(json::number::Number::U64(n)),
            })),
        }
    }
}

impl From<i64> for Json {
    fn from(n: i64) -> Self {
        Self {
            value: Some(json::Value::Number(json::Number {
                number: Some(json::number::Number::I64(n)),
            })),
        }
    }
}

impl From<f64> for Json {
    fn from(n: f64) -> Self {
        Self {
            value: Some(json::Value::Number(json::Number {
                number: Some(json::number::Number::F64(n)),
            })),
        }
    }
}

impl From<bool> for Json {
    fn from(b: bool) -> Self {
        Self {
            value: Some(json::Value::Bool(b)),
        }
    }
}

impl<T> From<Option<T>> for Json
where
    T: Into<Json>,
{
    fn from(opt: Option<T>) -> Self {
        match opt {
            Some(v) => v.into(),
            None => Self {
                value: Some(json::Value::Null(())),
            },
        }
    }
}

impl<K, V> FromIterator<(K, V)> for Json
where
    K: Into<String>,
    V: Into<Json>,
{
    fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
        Json {
            value: Some(json::Value::Object(json::Object {
                map: iter.into_iter().map(|(k, v)| (k.into(), v.into())).collect(),
            })),
        }
    }
}

impl<I> FromIterator<I> for Json
where
    I: Into<Json>,
{
    fn from_iter<T: IntoIterator<Item = I>>(iter: T) -> Self {
        Json {
            value: Some(json::Value::Array(json::Array {
                array: iter.into_iter().map(|i| i.into()).collect(),
            })),
        }
    }
}

impl From<Value> for Json {
    fn from(value: Value) -> Self {
        Self {
            value: Some(match value {
                Value::Null => json::Value::Null(()),
                Value::Bool(b) => json::Value::Bool(b),
                Value::Number(n) => json::Value::Number(json::Number {
                    number: n
                        .as_i64()
                        .map(json::number::Number::I64)
                        .or_else(|| n.as_u64().map(json::number::Number::U64))
                        .or_else(|| n.as_f64().map(json::number::Number::F64)),
                }),
                Value::String(s) => json::Value::String(s),
                Value::Array(a) => json::Value::Array(json::Array {
                    array: a.into_iter().map(Json::from).collect(),
                }),
                Value::Object(o) => json::Value::Object(json::Object {
                    map: o.into_iter().map(|(k, v)| (k, Json::from(v))).collect(),
                }),
            }),
        }
    }
}

impl From<Json> for Value {
    fn from(json: Json) -> Self {
        match json.value {
            Some(json::Value::Null(_)) | None => Value::Null,
            Some(json::Value::Bool(b)) => b.into(),
            Some(json::Value::Number(n)) => match n.number {
                Some(json::number::Number::I64(i)) => i.into(),
                Some(json::number::Number::U64(u)) => u.into(),
                Some(json::number::Number::F64(f)) => f.into(),
                None => Value::Null,
            },
            Some(json::Value::String(s)) => s.into(),
            Some(json::Value::Array(a)) => Value::Array(a.array.into_iter().map(Value::from).collect()),
            Some(json::Value::Object(o)) => Value::Object(
                o.map
                    .into_iter()
                    .map(|(key, value)| (key, Value::from(value)))
                    .collect(),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn into_value() {
        let type_hint_value = |x: Value| x;
        let type_hint_json = |x: Json| x;

        assert_eq!(type_hint_value(Json { value: None }.into()), json! {null});
        assert_eq!(
            type_hint_value(
                Json {
                    value: Some(json::Value::String("hello".into()))
                }
                .into()
            ),
            json! {"hello"}
        );
        assert_eq!(
            type_hint_value(
                Json::from_iter([
                    ("null".to_string(), type_hint_json(None::<bool>.into())),
                    ("bool".to_string(), type_hint_json(true.into())),
                    ("i64".to_string(), type_hint_json((-123_i64).into())),
                    ("u64".to_string(), type_hint_json(123_u64.into())),
                    ("f64".to_string(), type_hint_json(1.2_f64.into())),
                    ("string".to_string(), type_hint_json("value".to_string().into())),
                    (
                        "array".to_string(),
                        Json::from_iter(["foo".to_string(), "bar".to_string(), "baz".to_string()])
                    ),
                ])
                .into()
            ),
            json! {{"null": null, "bool": true, "i64": -123, "u64": 123, "f64": 1.2, "string": "value", "array": ["foo", "bar", "baz"]}}
        );
    }
}
