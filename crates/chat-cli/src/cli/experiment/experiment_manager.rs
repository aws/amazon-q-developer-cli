use crate::cli::chat::{
    ChatError,
    ChatSession,
};
use crate::database::settings::Setting;
use crate::os::Os;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExperimentName {
    Knowledge,
    Thinking,
    TangentMode,
    TodoList,
    Checkpoint,
    ContextUsageIndicator,
}

impl ExperimentName {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Knowledge => "Knowledge",
            Self::Thinking => "Thinking",
            Self::TangentMode => "Tangent Mode",
            Self::TodoList => "Todo Lists",
            Self::Checkpoint => "Checkpoint",
            Self::ContextUsageIndicator => "Context Usage Indicator",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Experiment {
    pub experiment_name: ExperimentName,
    pub description: &'static str,
    pub setting_key: Setting,
    pub enabled: bool,
    pub commands: &'static [&'static str],
}

static AVAILABLE_EXPERIMENTS: &[Experiment] = &[
    Experiment {
        experiment_name: ExperimentName::Knowledge,
        description: "Enables persistent context storage and retrieval across chat sessions (/knowledge)",
        setting_key: Setting::EnabledKnowledge,
        enabled: true,
        commands: &[
            "/knowledge",
            "/knowledge help",
            "/knowledge show",
            "/knowledge add",
            "/knowledge remove",
            "/knowledge clear",
            "/knowledge search",
            "/knowledge update",
            "/knowledge status",
            "/knowledge cancel",
        ],
    },
    Experiment {
        experiment_name: ExperimentName::Thinking,
        description: "Enables complex reasoning with step-by-step thought processes",
        setting_key: Setting::EnabledThinking,
        enabled: true,
        commands: &[],
    },
    Experiment {
        experiment_name: ExperimentName::TangentMode,
        description: "Enables entering into a temporary mode for sending isolated conversations (/tangent)",
        setting_key: Setting::EnabledTangentMode,
        enabled: true,
        commands: &["/tangent", "/tangent tail"],
    },
    Experiment {
        experiment_name: ExperimentName::TodoList,
        description: "Enables Q to create todo lists that can be viewed and managed using /todos",
        setting_key: Setting::EnabledTodoList,
        enabled: true,
        commands: &[
            "/todos",
            "/todos help",
            "/todos clear-finished",
            "/todos resume",
            "/todos view",
            "/todos delete",
            "/todos delete --all",
        ],
    },
    Experiment {
        experiment_name: ExperimentName::Checkpoint,
        description: "Enables workspace checkpoints to snapshot, list, expand, diff, and restore files (/checkpoint)\nNote: Cannot be used in tangent mode (to avoid mixing up conversation history)",
        setting_key: Setting::EnabledCheckpoint,
        enabled: true,
        commands: &[
            "/checkpoint",
            "/checkpoint help",
            "/checkpoint init",
            "/checkpoint list",
            "/checkpoint restore",
            "/checkpoint expand",
            "/checkpoint diff",
            "/checkpoint clean",
        ],
    },
    Experiment {
        experiment_name: ExperimentName::ContextUsageIndicator,
        description: "Shows context usage percentage in the prompt (e.g., [rust-agent] 6% >)",
        setting_key: Setting::EnabledContextUsageIndicator,
        enabled: true,
        commands: &[],
    },
];

pub struct ExperimentManager;

impl ExperimentManager {
    /// Checks if an experiment is enabled
    /// Returns false if experiment is disabled or not found
    pub fn is_enabled(os: &Os, experiment_type: ExperimentName) -> bool {
        let experiment = AVAILABLE_EXPERIMENTS
            .iter()
            .find(|exp| exp.experiment_name == experiment_type);
        match experiment {
            // Here we try to get value from storage, ONLY if experiment is enabled, otherwise we default
            // to false.
            Some(exp) if exp.enabled => os.database.settings.get_bool(exp.setting_key).unwrap_or(false),
            _ => false,
        }
    }

    /// Sets enabled state of experiment
    /// Returns false if experiment is disabled or not found
    pub async fn set_enabled(
        os: &mut Os,
        experiment_type: ExperimentName,
        enabled: bool,
        session: &mut ChatSession,
    ) -> Result<(), ChatError> {
        let experiment = AVAILABLE_EXPERIMENTS
            .iter()
            .find(|exp| exp.experiment_name == experiment_type);
        let setting = match experiment {
            Some(exp) => exp.setting_key,
            None => return Err(ChatError::Custom("Unknown experiment".into())),
        };

        os.database
            .settings
            .set(setting, enabled)
            .await
            .map_err(|e| ChatError::Custom(format!("Failed to update experiment setting: {e}").into()))?;
        // Makes sure tools are hot-reloaded, so the ones behind experiment flags are enabled.
        session.reload_builtin_tools(os).await?;

        Ok(())
    }

    // Returns all list of available experiments, with enabled state.
    pub fn get_experiments() -> Vec<&'static Experiment> {
        AVAILABLE_EXPERIMENTS.iter().filter(|exp| exp.enabled).collect()
    }

    /// Returns all commands from enabled experiments
    pub fn get_commands(os: &Os) -> Vec<&'static str> {
        AVAILABLE_EXPERIMENTS
            .iter()
            .filter(|exp| exp.enabled && Self::is_enabled(os, exp.experiment_name))
            .flat_map(|exp| exp.commands.iter())
            .copied()
            .collect()
    }
}
