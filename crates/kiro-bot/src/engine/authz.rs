//! Cedar policy authorization.
//!
//! Evaluates Cedar policies to control which users can interact with the bot,
//! switch agents, or use specific models. Supports policy templates with
//! slot-based linking for reusable access patterns.

use std::collections::HashMap;
use std::str::FromStr;

use anyhow::Result;
use cedar_policy::*;
use tracing::{
    debug,
    info,
};

use crate::engine::core::Conversation;

/// Evaluates Cedar authorization policies for bot access control.
pub struct Authorizer {
    authorizer: cedar_policy::Authorizer,
    policies: PolicySet,
    entities: Entities,
}

impl Authorizer {
    /// Load policies from a Cedar file, optionally linking templates and entities.
    pub fn new(policy_path: &str, template_values_path: Option<&str>, entities_path: Option<&str>) -> Result<Self> {
        debug!("Loading Cedar policy from: {}", policy_path);
        let policy_src = std::fs::read_to_string(policy_path)
            .map_err(|e| anyhow::anyhow!("Failed to read policy file '{}': {}", policy_path, e))?;

        let mut template_set = PolicySet::from_str(&policy_src)
            .map_err(|e| anyhow::anyhow!("Failed to parse Cedar policy in '{}':\n{}", policy_path, e))?;

        info!(
            policies = template_set.policies().count(),
            templates = template_set.templates().count(),
            "Loaded Cedar policies"
        );

        // Resolve template values: explicit arg > env var
        let tv_path = template_values_path
            .map(|s| s.to_string())
            .or_else(|| std::env::var("CEDAR_TEMPLATE_VALUES").ok());

        if let Some(values_path) = tv_path {
            let values_src = std::fs::read_to_string(&values_path)
                .map_err(|e| anyhow::anyhow!("Failed to read template values '{}': {}", values_path, e))?;
            let values: HashMap<String, HashMap<String, String>> = serde_json::from_str(&values_src)
                .map_err(|e| anyhow::anyhow!("Failed to parse template values JSON in '{}':\n{}", values_path, e))?;

            for (template_id_str, slot_values) in values {
                let template_id = PolicyId::from_str(&template_id_str)
                    .map_err(|e| anyhow::anyhow!("Invalid policy ID '{}': {}", template_id_str, e))?;
                let linked_id = PolicyId::from_str(&format!("linked_{}", template_id_str))
                    .map_err(|e| anyhow::anyhow!("Failed to create linked policy ID: {}", e))?;

                let mut slots = HashMap::new();
                for (slot_name, entity_uid_str) in slot_values {
                    let slot_id = match slot_name.as_str() {
                        "principal" => SlotId::principal(),
                        "resource" => SlotId::resource(),
                        _ => {
                            return Err(anyhow::anyhow!(
                                "Unsupported slot name '{}' in policy '{}'",
                                slot_name,
                                template_id_str
                            ));
                        },
                    };
                    let entity_uid = EntityUid::from_str(&entity_uid_str)
                        .map_err(|e| anyhow::anyhow!("Invalid entity UID '{}': {}", entity_uid_str, e))?;
                    slots.insert(slot_id, entity_uid);
                }

                template_set
                    .link(template_id.clone(), linked_id.clone(), slots)
                    .map_err(|e| anyhow::anyhow!("Failed to link template '{}': {}", template_id_str, e))?;
                info!("Linked template '{}'", template_id);
            }
        }

        let ep_path = entities_path
            .map(|s| s.to_string())
            .or_else(|| std::env::var("CEDAR_ENTITIES_FILE").ok());

        let entities = if let Some(ep) = ep_path {
            let src = std::fs::read_to_string(&ep)
                .map_err(|e| anyhow::anyhow!("Failed to read entities file '{}': {}", ep, e))?;
            Entities::from_json_str(&src, None)
                .map_err(|e| anyhow::anyhow!("Failed to parse entities JSON in '{}':\n{:?}", ep, e))?
        } else {
            Entities::empty()
        };

        Ok(Self {
            authorizer: cedar_policy::Authorizer::new(),
            policies: template_set,
            entities,
        })
    }

    fn check(
        &self,
        principal_id: &str,
        action_id: &str,
        resource_type: &str,
        resource_id: &str,
        context: Context,
    ) -> Result<bool> {
        let principal =
            EntityUid::from_type_name_and_id(EntityTypeName::from_str("User")?, EntityId::from_str(principal_id)?);
        let action =
            EntityUid::from_type_name_and_id(EntityTypeName::from_str("Action")?, EntityId::from_str(action_id)?);
        let resource = EntityUid::from_type_name_and_id(
            EntityTypeName::from_str(resource_type)?,
            EntityId::from_str(resource_id)?,
        );
        let request = Request::new(principal.clone(), action.clone(), resource.clone(), context, None)?;
        let response = self.authorizer.is_authorized(&request, &self.policies, &self.entities);
        debug!(
            %principal, %action, %resource,
            decision = ?response.decision(),
            "Authorization check"
        );
        Ok(response.decision() == Decision::Allow)
    }

    /// Check if a user can interact with the bot in a given conversation.
    pub fn can_use_bot(&self, user: &str, conversation: &Conversation) -> Result<bool> {
        // Surface the conversation kind via context so policies can use
        // `context.is_dm` without needing a full Entities table loaded.
        let is_dm = matches!(conversation, Conversation::Dm { .. });
        let ctx = Context::from_pairs([("is_dm".into(), RestrictedExpression::new_bool(is_dm))])?;
        self.check(user, "use_bot", "Conversation", &conversation.authz_id(), ctx)
    }

    /// Check if a user can switch to a specific agent.
    pub fn can_use_agent(&self, user: &str, agent: &str, channel: &str) -> Result<bool> {
        let ctx = Context::from_pairs([("channel".into(), RestrictedExpression::new_string(channel.into()))])?;
        self.check(user, "use_agent", "Agent", agent, ctx)
    }

    /// Check if a user can switch to a specific model.
    pub fn can_use_model(&self, user: &str, model: &str) -> Result<bool> {
        self.check(user, "use_model", "Model", model, Context::empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::core::Conversation;

    /// The kiro-help bot's Cedar policy, as committed under
    /// crates/kiro-bot/kiro-help/policies/agents.cedar. We embed the same text
    /// and parse it directly rather than going through the file-loading
    /// constructor — that keeps the test self-contained without taking on a
    /// `tempfile` dev-dep just for one file write.
    ///
    /// It is a template, and the placeholder sits inside a Cedar string literal,
    /// so the raw file parses fine — it just denotes a channel id nothing can
    /// match. We substitute it exactly the way docker/entrypoint.sh does at
    /// container start so the allow/deny tests below discriminate on the channel
    /// rather than passing because no channel is allowed at all.
    const KIRO_HELP_POLICY_TEMPLATE: &str = include_str!("../../kiro-help/policies/agents.cedar");
    const TEST_CHANNEL_ID: &str = "C0B6ESS6CF9";

    fn kiro_help_policy() -> String {
        KIRO_HELP_POLICY_TEMPLATE.replace("${ALLOWED_CHANNEL_ID}", TEST_CHANNEL_ID)
    }

    fn load(policy_text: &str) -> Authorizer {
        let policies = PolicySet::from_str(policy_text).expect("parse cedar policy");
        // Construct directly so the test doesn't need a writable filesystem
        // for tempfiles (sandboxes may block /var/folders).
        Authorizer {
            authorizer: cedar_policy::Authorizer::new(),
            policies,
            entities: Entities::empty(),
        }
    }

    #[test]
    fn kiro_help_policy_parses() {
        let _ = load(&kiro_help_policy());
    }

    /// Both placeholders are the contract entrypoint.sh greps for; if someone
    /// bakes a real id back into either file, the container refuses to start.
    /// config.toml is checked here too because it is the likelier one to be
    /// hand-edited, and nothing else in the crate reads the committed copy.
    ///
    /// Exactly once, not merely present: the entrypoint substitutes globally, so
    /// a second copy of the token — in a comment, say — gets rewritten into a
    /// bogus channel id in the deployed file. That is easy to reintroduce while
    /// editing the header comment, so it is asserted rather than documented.
    #[test]
    fn kiro_help_config_keeps_the_substitution_placeholders() {
        assert_eq!(
            KIRO_HELP_POLICY_TEMPLATE.matches("${ALLOWED_CHANNEL_ID}").count(),
            1,
            "agents.cedar must contain the placeholder exactly once — entrypoint.sh substitutes globally"
        );
        assert_eq!(
            include_str!("../../kiro-help/config.toml")
                .matches("${BOT_MEMBER_ID}")
                .count(),
            1,
            "config.toml must contain the placeholder exactly once — entrypoint.sh substitutes globally"
        );
    }

    #[test]
    fn kiro_help_policy_allows_dms() {
        let authz = load(&kiro_help_policy());
        let convo = Conversation::Dm {
            channel: "D123".to_string(),
            user: "U_USER".to_string(),
            thread_ts: "111.222".to_string(),
        };
        assert!(
            authz.can_use_bot("U_USER", &convo).unwrap(),
            "DM conversations must be allowed"
        );
    }

    #[test]
    fn kiro_help_policy_allows_the_substituted_channel() {
        let authz = load(&kiro_help_policy());
        let allowed = Conversation::Channel(TEST_CHANNEL_ID.to_string());
        assert!(authz.can_use_bot("U_USER", &allowed).unwrap());
    }

    #[test]
    fn kiro_help_policy_denies_unlisted_channels() {
        let authz = load(&kiro_help_policy());
        let other = Conversation::Channel("C_RANDOM".to_string());
        assert!(
            !authz.can_use_bot("U_USER", &other).unwrap(),
            "Unlisted channels must be denied"
        );
    }

    #[test]
    fn kiro_help_policy_inherits_channel_access_for_threads() {
        let authz = load(&kiro_help_policy());
        // Threads share their parent channel's access (authz_id projects to
        // 'channel:<id>'). Allowed-channel thread is permitted; random is not.
        let allowed_thread = Conversation::Thread {
            channel: TEST_CHANNEL_ID.to_string(),
            thread_ts: "1700000000.000".to_string(),
        };
        let random_thread = Conversation::Thread {
            channel: "C_RANDOM".to_string(),
            thread_ts: "1700000000.000".to_string(),
        };
        assert!(authz.can_use_bot("U_USER", &allowed_thread).unwrap());
        assert!(!authz.can_use_bot("U_USER", &random_thread).unwrap());
    }
}
