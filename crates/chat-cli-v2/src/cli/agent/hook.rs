use std::collections::HashMap;
use std::fmt::Display;

use schemars::JsonSchema;
use serde::{
    Deserialize,
    Deserializer,
    Serialize,
    Serializer,
};

const DEFAULT_TIMEOUT_MS: u64 = 30_000;
/// Timeout default (ms) for hooks read from the KAS array form. Matches KAS's own
/// default so an omitted `timeout` round-trips identically across engines; the
/// object form keeps the CLI's legacy 30s default.
const ARRAY_DEFAULT_TIMEOUT_MS: u64 = 10_000;
const DEFAULT_MAX_OUTPUT_SIZE: usize = 1024 * 10;
const DEFAULT_CACHE_TTL_SECONDS: u64 = 0;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq, JsonSchema, Hash)]
#[serde(rename_all = "camelCase")]
pub enum HookTrigger {
    /// Triggered during agent spawn
    AgentSpawn,
    /// Triggered per user message submission
    UserPromptSubmit,
    /// Triggered before tool execution
    PreToolUse,
    /// Triggered after tool execution
    PostToolUse,
    /// Triggered when the assistant finishes responding
    Stop,
}

impl Display for HookTrigger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HookTrigger::AgentSpawn => write!(f, "agentSpawn"),
            HookTrigger::UserPromptSubmit => write!(f, "userPromptSubmit"),
            HookTrigger::PreToolUse => write!(f, "preToolUse"),
            HookTrigger::PostToolUse => write!(f, "postToolUse"),
            HookTrigger::Stop => write!(f, "stop"),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Hash, Default)]
pub enum Source {
    #[default]
    Agent,
    Session,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, JsonSchema, Hash)]
pub struct Hook {
    /// The command to run when the hook is triggered
    pub command: String,

    /// Max time the hook can run before it throws a timeout error
    #[serde(default = "Hook::default_timeout_ms")]
    pub timeout_ms: u64,

    /// Max output size of the hook before it is truncated
    #[serde(default = "Hook::default_max_output_size")]
    pub max_output_size: usize,

    /// How long the hook output is cached before it will be executed again
    #[serde(default = "Hook::default_cache_ttl_seconds")]
    pub cache_ttl_seconds: u64,

    /// Optional glob matcher for hook
    /// Currently used for matching tool name of PreToolUse and PostToolUse hook
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matcher: Option<String>,

    #[schemars(skip)]
    #[serde(default, skip_serializing)]
    pub source: Source,
}

impl Hook {
    pub fn new(command: String, source: Source) -> Self {
        Self {
            command,
            timeout_ms: Self::default_timeout_ms(),
            max_output_size: Self::default_max_output_size(),
            cache_ttl_seconds: Self::default_cache_ttl_seconds(),
            matcher: None,
            source,
        }
    }

    fn default_timeout_ms() -> u64 {
        DEFAULT_TIMEOUT_MS
    }

    fn default_max_output_size() -> usize {
        DEFAULT_MAX_OUTPUT_SIZE
    }

    fn default_cache_ttl_seconds() -> u64 {
        DEFAULT_CACHE_TTL_SECONDS
    }
}

/// The on-disk shape a [`HooksField`] was read from, so it can be written back
/// in the same shape (faithful round-trip).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum HookWireShape {
    /// Legacy CLI object keyed by trigger: `{ "agentSpawn": [ { "command": … } ] }`.
    #[default]
    Object,
    /// KAS array of hook documents: `[ { "name", "trigger", "action": { … } } ]`.
    Array,
}

/// Hooks configuration that accepts **both** the legacy CLI object form and the
/// KAS array form on read, and serializes back in whichever shape it was read
/// from. This lets a single agent file be "universal" — loadable by both the
/// Rust CLI and KAS — without changing KAS.
///
/// Internally it is just `HashMap<HookTrigger, Vec<Hook>>`; [`Deref`]/[`DerefMut`]
/// keep existing call sites unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HooksField {
    map: HashMap<HookTrigger, Vec<Hook>>,
    origin: HookWireShape,
}

impl HooksField {
    pub fn as_map(&self) -> &HashMap<HookTrigger, Vec<Hook>> {
        &self.map
    }

    /// Whether there are no hooks. Used to omit the field on serialize so a
    /// hooks-less agent stays loadable by both engines (an empty object is not
    /// valid KAS array form).
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Convert the internal map into KAS hook documents for array-form output.
    /// Ordering is deterministic (triggers sorted by their wire name) so
    /// round-trips are stable.
    fn to_wire_docs(&self) -> Vec<WireHookDocument> {
        let mut triggers: Vec<&HookTrigger> = self.map.keys().collect();
        triggers.sort_by_key(|t| t.to_string());
        let mut out = Vec::new();
        for trigger in triggers {
            for (idx, hook) in self.map[trigger].iter().enumerate() {
                out.push(WireHookDocument {
                    // KAS requires a non-empty name; the CLI has no hook name, so synthesize a
                    // stable one from the trigger + position.
                    name: Some(format!("{trigger}-{idx}")),
                    trigger: trigger.to_string(),
                    matcher: hook.matcher.clone(),
                    action: WireHookAction::Command {
                        command: hook.command.clone(),
                    },
                    // Always emit timeout (seconds) so a value equal to one engine's default is
                    // not dropped and re-read as a different engine's default.
                    timeout: Some(hook.timeout_ms.div_ceil(1000)),
                    // CLI-only extras: emit only when non-default (KAS strips unknown keys).
                    max_output_size: (hook.max_output_size != DEFAULT_MAX_OUTPUT_SIZE).then_some(hook.max_output_size),
                    cache_ttl_seconds: (hook.cache_ttl_seconds != DEFAULT_CACHE_TTL_SECONDS)
                        .then_some(hook.cache_ttl_seconds),
                    enabled: true,
                });
            }
        }
        out
    }
}

impl std::ops::Deref for HooksField {
    type Target = HashMap<HookTrigger, Vec<Hook>>;

    fn deref(&self) -> &Self::Target {
        &self.map
    }
}

impl std::ops::DerefMut for HooksField {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.map
    }
}

impl From<HashMap<HookTrigger, Vec<Hook>>> for HooksField {
    fn from(map: HashMap<HookTrigger, Vec<Hook>>) -> Self {
        Self {
            map,
            origin: HookWireShape::Object,
        }
    }
}

/// Normalizes a wire trigger string (CLI camelCase or KAS PascalCase) onto the
/// CLI's [`HookTrigger`]. Returns `None` for KAS-only triggers with no CLI
/// equivalent (e.g. `PostFileSave`); such hooks are skipped on load.
fn normalize_hook_trigger(s: &str) -> Option<HookTrigger> {
    match s {
        "agentSpawn" | "AgentSpawn" | "SessionStart" | "sessionStart" => Some(HookTrigger::AgentSpawn),
        "userPromptSubmit" | "UserPromptSubmit" => Some(HookTrigger::UserPromptSubmit),
        "preToolUse" | "PreToolUse" => Some(HookTrigger::PreToolUse),
        "postToolUse" | "PostToolUse" => Some(HookTrigger::PostToolUse),
        "stop" | "Stop" => Some(HookTrigger::Stop),
        _ => None,
    }
}

fn hook_enabled_default() -> bool {
    true
}

fn hook_enabled_is_default(enabled: &bool) -> bool {
    *enabled
}

/// A single KAS hook document (array-form element). Used for both reading KAS
/// files and serializing the CLI's hooks back to array form.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WireHookDocument {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    trigger: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    matcher: Option<String>,
    action: WireHookAction,
    /// Timeout in SECONDS (KAS unit).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    timeout: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    max_output_size: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cache_ttl_seconds: Option<u64>,
    #[serde(default = "hook_enabled_default", skip_serializing_if = "hook_enabled_is_default")]
    enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(crate) enum WireHookAction {
    Command {
        command: String,
    },
    /// Sends a prompt to the model. Unsupported by the CLI runtime (skipped on load).
    Agent {
        prompt: String,
    },
}

/// Schema-only representation documenting both accepted `hooks` shapes.
#[derive(JsonSchema)]
#[serde(untagged)]
#[allow(dead_code)]
pub(crate) enum HooksFieldSchema {
    Object(HashMap<HookTrigger, Vec<Hook>>),
    Array(Vec<WireHookDocument>),
}

impl<'de> Deserialize<'de> for HooksField {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Repr {
            Object(HashMap<HookTrigger, Vec<Hook>>),
            Array(Vec<WireHookDocument>),
        }

        match Repr::deserialize(deserializer)? {
            Repr::Object(map) => Ok(HooksField {
                map,
                origin: HookWireShape::Object,
            }),
            Repr::Array(docs) => {
                let mut map: HashMap<HookTrigger, Vec<Hook>> = HashMap::new();
                for doc in docs {
                    if !doc.enabled {
                        continue;
                    }
                    let Some(trigger) = normalize_hook_trigger(&doc.trigger) else {
                        tracing::warn!(trigger = %doc.trigger, "skipping hook with unsupported trigger");
                        continue;
                    };
                    let command = match doc.action {
                        WireHookAction::Command { command } => command,
                        WireHookAction::Agent { .. } => {
                            tracing::warn!(
                                trigger = %doc.trigger,
                                "skipping agent-type hook action unsupported by the CLI runtime"
                            );
                            continue;
                        },
                    };
                    let hook = Hook {
                        command,
                        timeout_ms: doc.timeout.map_or(ARRAY_DEFAULT_TIMEOUT_MS, |s| s.saturating_mul(1000)),
                        max_output_size: doc.max_output_size.unwrap_or(DEFAULT_MAX_OUTPUT_SIZE),
                        cache_ttl_seconds: doc.cache_ttl_seconds.unwrap_or(DEFAULT_CACHE_TTL_SECONDS),
                        matcher: doc.matcher,
                        source: Source::Agent,
                    };
                    map.entry(trigger).or_default().push(hook);
                }
                Ok(HooksField {
                    map,
                    origin: HookWireShape::Array,
                })
            },
        }
    }
}

impl Serialize for HooksField {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self.origin {
            HookWireShape::Object => self.map.serialize(serializer),
            HookWireShape::Array => self.to_wire_docs().serialize(serializer),
        }
    }
}
