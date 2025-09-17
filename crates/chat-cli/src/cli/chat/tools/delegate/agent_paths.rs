use std::path::PathBuf;

use eyre::Result;

use crate::os::Os;

pub struct AgentPaths;

impl AgentPaths {
    pub async fn subagents_dir(os: &Os) -> Result<PathBuf> {
        let home_dir = os.env.home().unwrap_or_default();
        let agents_dir = home_dir.join(".aws").join("amazonq").join(".subagents");

        if !agents_dir.exists() {
            std::fs::create_dir_all(&agents_dir)?;
        }

        Ok(agents_dir)
    }

    pub async fn cli_agents_dir(os: &Os) -> Result<PathBuf> {
        let home_dir = os.env.home().unwrap_or_default();
        Ok(home_dir.join(".aws").join("amazonq").join("cli-agents"))
    }

    pub async fn agent_file(os: &Os, agent: &str) -> Result<PathBuf> {
        let agents_dir = Self::subagents_dir(os).await?;
        Ok(agents_dir.join(format!("{}.json", agent)))
    }
}
