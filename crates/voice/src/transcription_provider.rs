#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub enum TranscriptionBackend {
    #[default]
    LocalWhisper,
    /// Remote voice server for cloud desktop use (no local microphone)
    RemoteServer { url: String },
}
