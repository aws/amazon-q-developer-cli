use serde::{
    Deserialize,
    Serialize,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct Feed {
    pub entries: Vec<Entry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    #[serde(rename = "type")]
    pub entry_type: String,
    pub date: String,
    pub version: String,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub changes: Vec<Change>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Change {
    #[serde(rename = "type")]
    pub change_type: String,
    pub description: String,
}

impl Feed {
    pub fn load() -> Self {
        serde_json::from_str(include_str!("./feed.json")).expect("feed.json is valid json")
    }

    pub fn get_version_changelog(&self, version: &str) -> Option<Entry> {
        self.entries
            .iter()
            .find(|entry| entry.entry_type == "release" && entry.version == version && !entry.hidden)
            .cloned()
    }

    pub fn get_all_changelogs(&self) -> Vec<Entry> {
        self.entries
            .iter()
            .filter(|entry| entry.entry_type == "release" && !entry.hidden)
            .cloned()
            .collect()
    }
}
