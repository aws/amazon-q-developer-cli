use anyhow::Result;

pub fn cleanup() -> Result<()> {
    // TODO: renable with ssh
    // if let Ok(parent) = std::env::var(Q_PARENT) {
    //     if !parent.is_empty() {
    //         trace!("Cleaning up parent file");
    //         let parent_path = directories::parent_socket_path(&parent)?;
    //         if parent_path.exists() {
    //             std::fs::remove_file(parent_path)?;
    //         }
    //     }
    // }

    Ok(())
}
