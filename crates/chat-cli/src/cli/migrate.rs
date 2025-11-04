use std::path::{
    Path,
    PathBuf,
};
use std::process::ExitCode;

use clap::Parser;
use eyre::{
    Context,
    Result,
};
use rustix::fs::{
    FlockOperation,
    flock,
};
use serde_json::{
    Map,
    Value,
};
use tokio::fs;
use tracing::debug;

use crate::os::Os;
use crate::util::paths::GlobalPaths;

#[derive(Debug, Parser, PartialEq)]
pub struct MigrateArgs {
    /// Force migration even if already completed
    #[arg(long)]
    pub force: bool,

    /// Dry run - show what would be migrated
    #[arg(long)]
    pub dry_run: bool,

    /// Skip confirmation prompts
    #[arg(long, short = 'y')]
    pub yes: bool,
}

impl MigrateArgs {
    pub async fn execute(self, _os: &mut Os) -> Result<ExitCode> {
        // Try to acquire migration lock
        let _lock = match acquire_migration_lock()? {
            Some(lock) => lock,
            None => {
                // Another process is migrating, skip silently
                return Ok(ExitCode::SUCCESS);
            },
        };

        if !self.yes && !self.dry_run {
            println!("This will migrate your database and settings from amazon-q to kiro-cli.");
            println!("\nMigration details:");
            println!("  • Database: {{data_local_dir}}/amazon-q → {{data_local_dir}}/kiro-cli");
            println!("  • Settings: {{data_local_dir}}/amazon-q → ~/.aws/kiro-cli");
            println!("\nContinue? (y/N)");

            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            if !input.trim().eq_ignore_ascii_case("y") {
                println!("Migration cancelled");
                return Ok(ExitCode::SUCCESS);
            }
        }

        let status = detect_migration()?;

        if !self.force && matches!(status, MigrationStatus::Completed) {
            debug!("✓ Migration already completed");
            return Ok(ExitCode::SUCCESS);
        }

        let MigrationStatus::Needed {
            old_db,
            old_settings,
            new_db,
            new_settings,
        } = status
        else {
            debug!("✓ No migration needed (fresh install)");
            return Ok(ExitCode::SUCCESS);
        };

        // Migrate database
        let db_result = migrate_database(&old_db, &new_db, self.dry_run).await?;
        debug!("✓ Database: {}", db_result.message);

        // Migrate settings
        let settings_result = migrate_settings(&old_settings, &new_settings, self.dry_run).await?;
        debug!("✓ Settings: {}", settings_result.message);
        if !settings_result.transformations.is_empty() {
            debug!("  Transformations applied:");
            for t in &settings_result.transformations {
                debug!("    - {t}");
            }
        }

        if !self.dry_run {
            debug!("\n✓ Migration completed successfully!");
        } else {
            println!("\n(Dry run - no changes made)");
        }

        Ok(ExitCode::SUCCESS)
    }
}

// Migration detection
#[derive(Debug)]
enum MigrationStatus {
    NotNeeded,
    Needed {
        old_db: PathBuf,
        old_settings: PathBuf,
        new_db: PathBuf,
        new_settings: PathBuf,
    },
    Completed,
}

fn detect_migration() -> Result<MigrationStatus> {
    let old_db = GlobalPaths::old_database_path()?;
    let old_settings = GlobalPaths::old_settings_path()?;
    let new_db = GlobalPaths::new_database_path()?;
    let new_settings = GlobalPaths::new_settings_path()?;

    let old_exists = old_db.exists() || old_settings.exists();
    let migration_completed = GlobalPaths::is_migration_completed_static().unwrap_or(false);

    if migration_completed {
        Ok(MigrationStatus::Completed)
    } else if old_exists {
        Ok(MigrationStatus::Needed {
            old_db,
            old_settings,
            new_db,
            new_settings,
        })
    } else {
        Ok(MigrationStatus::NotNeeded)
    }
}

// Database migration
#[derive(Debug)]
struct DbMigrationResult {
    message: String,
    #[allow(dead_code)]
    bytes_copied: u64,
}

async fn migrate_database(old_path: &Path, new_path: &Path, dry_run: bool) -> Result<DbMigrationResult> {
    if !old_path.exists() {
        return Ok(DbMigrationResult {
            message: "No database to migrate".to_string(),
            bytes_copied: 0,
        });
    }

    let metadata = fs::metadata(old_path).await.context("Cannot read source database")?;
    if !metadata.is_file() {
        eyre::bail!("Database is not a file");
    }
    if metadata.len() == 0 {
        eyre::bail!("Database is empty");
    }

    if dry_run {
        return Ok(DbMigrationResult {
            message: format!(
                "Would copy database:\n  From: {}\n  To: {}",
                old_path.display(),
                new_path.display()
            ),
            bytes_copied: 0,
        });
    }

    if let Some(parent) = new_path.parent() {
        fs::create_dir_all(parent)
            .await
            .context("Failed to create target directory")?;
    }

    let bytes = fs::copy(old_path, new_path).await.context("Failed to copy database")?;

    let metadata = fs::metadata(new_path).await.context("Cannot read migrated database")?;
    if metadata.len() == 0 {
        eyre::bail!("Migrated database is empty");
    }

    Ok(DbMigrationResult {
        message: format!("Migrated database ({bytes} bytes)"),
        bytes_copied: bytes,
    })
}

// Settings migration
#[derive(Debug)]
struct SettingsMigrationResult {
    message: String,
    #[allow(dead_code)]
    settings_count: usize,
    transformations: Vec<String>,
}

async fn migrate_settings(old_path: &Path, new_path: &Path, dry_run: bool) -> Result<SettingsMigrationResult> {
    let settings = if old_path.exists() {
        let content = fs::read_to_string(old_path)
            .await
            .context("Failed to read settings file")?;
        let value: Value = serde_json::from_str(&content).context("Failed to parse settings JSON")?;
        match value {
            Value::Object(map) => map,
            _ => Map::new(),
        }
    } else {
        Map::new()
    };

    let mut transformed = settings;
    let mut transformations = Vec::new();

    // Transform api.q.service → api.kiro.service
    if let Some(q_service) = transformed.remove("api.q.service") {
        transformed.insert("api.kiro.service".to_string(), q_service);
        transformations.push("api.q.service → api.kiro.service".to_string());
    }

    // Add migration completed flag
    transformed.insert("migration.kiro.completed".to_string(), Value::Bool(true));
    transformations.push("Added migration.kiro.completed flag".to_string());

    if dry_run {
        return Ok(SettingsMigrationResult {
            message: format!(
                "Would migrate settings:\n  From: {}\n  To: {}\n  Transformations: {}",
                old_path.display(),
                new_path.display(),
                transformations.join(", ")
            ),
            settings_count: transformed.len(),
            transformations,
        });
    }

    if let Some(parent) = new_path.parent() {
        fs::create_dir_all(parent)
            .await
            .context("Failed to create target directory")?;
    }

    let json = serde_json::to_string_pretty(&transformed).context("Failed to serialize settings")?;
    fs::write(new_path, json)
        .await
        .context("Failed to write settings file")?;

    Ok(SettingsMigrationResult {
        message: format!("Settings migrated successfully ({} settings)", transformed.len()),
        settings_count: transformed.len(),
        transformations,
    })
}

// File locking
fn acquire_migration_lock() -> Result<Option<std::fs::File>> {
    let lock_path = GlobalPaths::migration_lock_path()?;

    if let Some(parent) = lock_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(lock_path)?;

    match flock(&file, FlockOperation::NonBlockingLockExclusive) {
        Ok(()) => Ok(Some(file)),
        Err(_) => Ok(None),
    }
}
