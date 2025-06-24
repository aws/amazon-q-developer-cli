use thiserror::Error;

#[derive(Debug, Error)]
pub enum GetShellError {
    #[error("not yet implemented for windows")]
    NoParent,
}

pub fn get_shell() -> Result<String, GetShellError> {
    fig_util::get_parent_process_exe()
        .ok_or(GetShellError::NoParent)
        .map(|path| path.to_string_lossy().to_string())
}
