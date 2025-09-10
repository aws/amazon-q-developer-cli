use std::path::Path;

use eyre::Result;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::os::Os;

pub async fn load_json<T: DeserializeOwned>(os: &Os, path: &Path) -> Result<Option<T>> {
    if !path.exists() {
        return Ok(None);
    }

    let content = os.fs.read_to_string(path).await?;
    let data: T = serde_json::from_str(&content)?;
    Ok(Some(data))
}

pub async fn save_json<T: Serialize>(os: &Os, path: &Path, data: &T) -> Result<()> {
    let content = serde_json::to_string_pretty(data)?;
    os.fs.write(path, content).await?;
    Ok(())
}
