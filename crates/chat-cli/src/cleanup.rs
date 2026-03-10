use std::time::{
    Duration,
    SystemTime,
};

use tracing::debug;

use crate::database::Database;
use crate::database::settings::Setting;
use crate::os::{
    Env,
    Fs,
};
use crate::util::paths::{
    DirectoryError,
    PathResolver,
};

/// Cleanup old data based on the cleanupPeriodDays setting.
/// Runs before user interaction to avoid race conditions.
pub async fn cleanup_old_data(env: &Env, fs: &Fs, database: &Database) -> Result<(), DirectoryError> {
    let Some(days) = database.settings.get_int(Setting::CleanupPeriodDays) else {
        return Ok(());
    };

    if days < 0 {
        return Ok(());
    }

    let cutoff = if days == 0 {
        SystemTime::now()
    } else {
        let seconds = u64::try_from(days)
            .ok()
            .and_then(|d| d.checked_mul(86400))
            .ok_or_else(|| {
                DirectoryError::Io(std::io::Error::other(format!(
                    "Cleanup period too large: {} days",
                    days
                )))
            })?;

        SystemTime::now()
            .checked_sub(Duration::from_secs(seconds))
            .ok_or_else(|| DirectoryError::Io(std::io::Error::other("Cutoff time calculation underflow")))?
    };

    let cutoff_ms = i64::try_from(
        cutoff
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
    )
    .unwrap_or(i64::MAX);

    debug!("Cleaning up data older than {} days", days);

    let resolver = PathResolver::new(env, fs);
    let global = resolver.global();

    let sessions_dir = global.sessions_dir()?;
    cleanup_sessions(&sessions_dir, cutoff).await?;
    cleanup_knowledge_bases(&global.knowledge_bases_dir()?, cutoff).await?;
    cleanup_conversations(database, cutoff_ms)?;

    Ok(())
}

async fn cleanup_sessions(sessions_dir: &std::path::Path, cutoff: SystemTime) -> Result<(), DirectoryError> {
    if !sessions_dir.exists() {
        return Ok(());
    }

    // Track the newest mtime per session ID so we only delete sessions
    // where *all* files are older than the cutoff.
    let mut session_newest: std::collections::HashMap<String, SystemTime> = std::collections::HashMap::new();
    let mut entries = tokio::fs::read_dir(sessions_dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if let Some(ext) = path.extension()
            && (ext == "json" || ext == "jsonl")
            && let Ok(metadata) = tokio::fs::metadata(&path).await
            && let Ok(modified) = metadata.modified()
            && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
        {
            session_newest
                .entry(stem.to_string())
                .and_modify(|prev| {
                    if modified > *prev {
                        *prev = modified;
                    }
                })
                .or_insert(modified);
        }
    }

    let old_session_ids: std::collections::HashSet<_> = session_newest
        .into_iter()
        .filter(|(_, newest)| *newest < cutoff)
        .map(|(id, _)| id)
        .collect();

    // Delete each old session, skipping any that are actively locked
    for session_id in &old_session_ids {
        match chat_cli_v2::agent::session::acquire_lock(sessions_dir, session_id) {
            Ok(_guard) => {
                debug!("Deleting old session: {}", session_id);
                for ext in ["json", "jsonl"] {
                    let path = sessions_dir.join(format!("{}.{}", session_id, ext));
                    if let Err(e) = tokio::fs::remove_file(&path).await
                        && e.kind() != std::io::ErrorKind::NotFound
                    {
                        tracing::warn!("Failed to delete {}: {}", path.display(), e);
                    }
                }
                // _guard drops here, removing the .lock file
            },
            Err(chat_cli_v2::agent::session::SessionError::ActiveSession { pid, .. }) => {
                debug!("Skipping active session {} (PID {})", session_id, pid);
            },
            Err(e) => {
                tracing::warn!("Skipping session {} (lock error: {})", session_id, e);
            },
        }
    }

    Ok(())
}

async fn cleanup_knowledge_bases(kb_dir: &std::path::Path, cutoff: SystemTime) -> Result<(), DirectoryError> {
    if !kb_dir.exists() {
        return Ok(());
    }

    let mut entries = tokio::fs::read_dir(kb_dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if let Ok(metadata) = tokio::fs::metadata(&path).await
            && metadata.is_dir()
            && newest_mtime_in_dir(&path).await.is_some_and(|t| t < cutoff)
        {
            debug!("Deleting old knowledge base: {}", path.display());
            if let Err(e) = tokio::fs::remove_dir_all(&path).await {
                tracing::warn!("Failed to delete {}: {}", path.display(), e);
            }
        }
    }

    Ok(())
}

/// Walk a directory and return the most recent mtime of any file within it.
async fn newest_mtime_in_dir(dir: &std::path::Path) -> Option<SystemTime> {
    let mut newest: Option<SystemTime> = None;
    let mut stack = vec![dir.to_path_buf()];

    while let Some(current) = stack.pop() {
        let Ok(mut entries) = tokio::fs::read_dir(&current).await else {
            tracing::warn!("Failed to read directory: {}", current.display());
            return None;
        };
        loop {
            match entries.next_entry().await {
                Ok(Some(entry)) => {
                    let path = entry.path();
                    if let Ok(metadata) = tokio::fs::metadata(&path).await {
                        if metadata.is_dir() {
                            stack.push(path);
                        } else if let Ok(modified) = metadata.modified() {
                            newest = Some(match newest {
                                Some(prev) if prev >= modified => prev,
                                _ => modified,
                            });
                        }
                    }
                },
                Ok(None) => break,
                Err(e) => {
                    tracing::warn!("Error reading directory entry in {}: {}", current.display(), e);
                    return None;
                },
            }
        }
    }

    newest.or_else(|| std::fs::metadata(dir).ok()?.modified().ok())
}

fn cleanup_conversations(database: &Database, cutoff_ms: i64) -> Result<(), DirectoryError> {
    match database.delete_conversations_older_than(cutoff_ms) {
        Ok(count) if count > 0 => debug!("Deleted {} old conversations from database", count),
        Ok(_) => {},
        Err(e) => return Err(DirectoryError::Io(std::io::Error::other(e.to_string()))),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::{
        Duration,
        SystemTime,
    };

    use super::*;

    fn set_mtime(path: &std::path::Path, time: SystemTime) {
        let file = std::fs::File::options().write(true).open(path).unwrap();
        file.set_times(std::fs::FileTimes::new().set_modified(time)).unwrap();
    }

    // --- cleanup_sessions tests ---

    #[tokio::test]
    async fn sessions_nonexistent_dir_is_noop() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("does-not-exist");
        cleanup_sessions(&missing, SystemTime::now()).await.unwrap();
    }

    #[tokio::test]
    async fn sessions_deletes_old_preserves_new() {
        let dir = tempfile::tempdir().unwrap();
        let old_time = SystemTime::now() - Duration::from_secs(200);
        let cutoff = SystemTime::now() - Duration::from_secs(100);

        // Old files that should be deleted
        let old_json = dir.path().join("old.json");
        let old_jsonl = dir.path().join("old.jsonl");
        std::fs::write(&old_json, "{}").unwrap();
        std::fs::write(&old_jsonl, "{}").unwrap();
        set_mtime(&old_json, old_time);
        set_mtime(&old_jsonl, old_time);

        // New files that should be preserved
        let new_json = dir.path().join("new.json");
        std::fs::write(&new_json, "{}").unwrap();

        // Non-session file that should be preserved regardless of age
        let old_txt = dir.path().join("old.txt");
        std::fs::write(&old_txt, "").unwrap();
        set_mtime(&old_txt, old_time);

        cleanup_sessions(dir.path(), cutoff).await.unwrap();

        assert!(!old_json.exists());
        assert!(!old_jsonl.exists());
        assert!(new_json.exists());
        assert!(old_txt.exists());
    }

    #[tokio::test]
    async fn sessions_preserves_session_with_recent_log() {
        let dir = tempfile::tempdir().unwrap();
        let old_time = SystemTime::now() - Duration::from_secs(200);
        let recent_time = SystemTime::now() - Duration::from_secs(50);
        let cutoff = SystemTime::now() - Duration::from_secs(100);

        // Session where .json is old but .jsonl was recently appended — should be preserved
        let json = dir.path().join("active.json");
        let jsonl = dir.path().join("active.jsonl");
        std::fs::write(&json, "{}").unwrap();
        std::fs::write(&jsonl, "{}").unwrap();
        set_mtime(&json, old_time);
        set_mtime(&jsonl, recent_time);

        cleanup_sessions(dir.path(), cutoff).await.unwrap();

        assert!(json.exists());
        assert!(jsonl.exists());
    }

    // --- cleanup_knowledge_bases tests ---

    #[tokio::test]
    async fn kb_nonexistent_dir_is_noop() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("does-not-exist");
        cleanup_knowledge_bases(&missing, SystemTime::now()).await.unwrap();
    }

    #[tokio::test]
    async fn kb_uses_newest_file_mtime() {
        let dir = tempfile::tempdir().unwrap();
        let old_time = SystemTime::now() - Duration::from_secs(200);
        let recent_time = SystemTime::now() - Duration::from_secs(50);
        let cutoff = SystemTime::now() - Duration::from_secs(100);

        // KB with a recently-modified file inside — should be preserved
        let active_kb = dir.path().join("active-kb");
        std::fs::create_dir(&active_kb).unwrap();
        let old_file = active_kb.join("index.bin");
        let new_file = active_kb.join("cache.bin");
        std::fs::write(&old_file, "data").unwrap();
        std::fs::write(&new_file, "data").unwrap();
        set_mtime(&old_file, old_time);
        set_mtime(&new_file, recent_time);

        // KB with only old files — should be deleted
        let stale_kb = dir.path().join("stale-kb");
        std::fs::create_dir(&stale_kb).unwrap();
        let stale_file = stale_kb.join("index.bin");
        std::fs::write(&stale_file, "data").unwrap();
        set_mtime(&stale_file, old_time);

        cleanup_knowledge_bases(dir.path(), cutoff).await.unwrap();

        assert!(active_kb.exists());
        assert!(!stale_kb.exists());
    }

    #[tokio::test]
    async fn kb_checks_nested_files() {
        let dir = tempfile::tempdir().unwrap();
        let old_time = SystemTime::now() - Duration::from_secs(200);
        let recent_time = SystemTime::now() - Duration::from_secs(50);
        let cutoff = SystemTime::now() - Duration::from_secs(100);

        // KB with a recent file in a subdirectory — should be preserved
        let kb = dir.path().join("nested-kb");
        let subdir = kb.join("subdir");
        std::fs::create_dir_all(&subdir).unwrap();
        let old_file = kb.join("old.bin");
        let nested_new = subdir.join("new.bin");
        std::fs::write(&old_file, "data").unwrap();
        std::fs::write(&nested_new, "data").unwrap();
        set_mtime(&old_file, old_time);
        set_mtime(&nested_new, recent_time);

        cleanup_knowledge_bases(dir.path(), cutoff).await.unwrap();

        assert!(kb.exists());
    }

    #[tokio::test]
    async fn kb_skips_non_directories() {
        let dir = tempfile::tempdir().unwrap();
        let old_time = SystemTime::now() - Duration::from_secs(200);
        let cutoff = SystemTime::now() - Duration::from_secs(100);

        // A plain file in the KB directory — should not be deleted
        let file = dir.path().join("stray-file.txt");
        std::fs::write(&file, "data").unwrap();
        set_mtime(&file, old_time);

        cleanup_knowledge_bases(dir.path(), cutoff).await.unwrap();

        assert!(file.exists());
    }

    // --- cleanup_conversations tests ---

    #[tokio::test]
    async fn conversations_deletes_old_preserves_new() {
        let db = Database::new_default().await.unwrap();

        let now_ms = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let old_ms = now_ms - 200_000;
        let cutoff_ms = now_ms - 100_000;

        let conv_json = r#"{"conversation_id":"placeholder","next_message":null,"history":[],"valid_history_range":[0,0],"transcript":[],"tools":{},"context_manager":null,"context_message_length":null,"latest_summary":null,"model_info":null,"file_line_tracker":{},"checkpoint_manager":null,"mcp_enabled":true,"user_turn_metadata":{"continuation_id":"test","requests":[],"usage_info":[]}}"#;

        // Insert old conversation
        let old_json = conv_json.replace("placeholder", "old-conv");
        db.insert_conversation_with_timestamp("/old/path", "old-conv", &old_json, old_ms)
            .unwrap();

        // Insert new conversation
        let new_json = conv_json.replace("placeholder", "new-conv");
        db.insert_conversation_with_timestamp("/new/path", "new-conv", &new_json, now_ms)
            .unwrap();

        cleanup_conversations(&db, cutoff_ms).unwrap();

        assert!(db.get_conversation_by_id("old-conv").unwrap().is_none());
        assert!(db.get_conversation_by_id("new-conv").unwrap().is_some());
    }

    // --- cleanup_old_data tests ---

    #[tokio::test]
    async fn cleanup_noop_when_setting_unset() {
        let env = Env::new();
        let fs = Fs::new();
        let db = Database::new_default().await.unwrap();
        // Setting not configured — should be a no-op
        cleanup_old_data(&env, &fs, &db).await.unwrap();
    }

    #[tokio::test]
    async fn cleanup_noop_when_setting_negative() {
        let env = Env::new();
        let fs = Fs::new();
        let mut db = Database::new_default().await.unwrap();
        db.settings.set(Setting::CleanupPeriodDays, -5, None).await.unwrap();
        // Negative values are a no-op (not an error)
        cleanup_old_data(&env, &fs, &db).await.unwrap();
    }

    #[tokio::test]
    async fn cleanup_zero_means_delete_everything() {
        let dir = tempfile::tempdir().unwrap();
        let sessions_dir = dir.path().join("sessions");
        std::fs::create_dir(&sessions_dir).unwrap();

        // Create a recent session file
        std::fs::write(sessions_dir.join("recent.json"), "{}").unwrap();

        let env = Env::from_slice(&[
            ("HOME", "/tmp/test-home"),
            ("KIRO_TEST_SESSIONS_DIR", sessions_dir.to_str().unwrap()),
        ]);
        let fs = Fs::new();
        let mut db = Database::new_default().await.unwrap();
        db.settings.set(Setting::CleanupPeriodDays, 0, None).await.unwrap();

        cleanup_old_data(&env, &fs, &db).await.unwrap();

        // 0 means cutoff = now, so even recent files are deleted
        assert!(!sessions_dir.join("recent.json").exists());
    }
}
