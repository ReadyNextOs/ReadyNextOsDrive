use crate::db::{self, DbPool, LocalTrashEntry};
use crate::error::{AppError, AppResult};
use chrono::{Duration, Utc};
use std::path::{Path, PathBuf};

/// Number of days to keep files in local trash before auto-cleanup.
const TRASH_RETENTION_DAYS: i64 = 30;

/// Local trash manager — moves deleted files to a trash directory
/// instead of permanently deleting them. Provides restore and cleanup.
pub struct LocalTrashManager {
    trash_dir: PathBuf,
    db: DbPool,
}

impl LocalTrashManager {
    /// Create a new trash manager.
    /// trash_dir: e.g. ~/.veloryn-trash/ (created if not exists)
    pub fn new(trash_dir: PathBuf, db: DbPool) -> Self {
        Self { trash_dir, db }
    }

    /// Move a file to the local trash instead of deleting it.
    /// Returns the trash path where the file was moved.
    pub fn trash_file(
        &self,
        file_path: &Path,
        sync_zone: &str,
    ) -> AppResult<PathBuf> {
        if !file_path.exists() {
            return Err(AppError::io(format!(
                "File does not exist: {}",
                file_path.display()
            )));
        }

        // Create date-based subdirectory: ~/.veloryn-trash/2026-04-09/
        let date_dir = Utc::now().format("%Y-%m-%d").to_string();
        let trash_subdir = self.trash_dir.join(&date_dir);
        std::fs::create_dir_all(&trash_subdir).map_err(|e| {
            AppError::io(format!("Cannot create trash directory: {}", e))
        })?;

        // Generate unique trash filename to avoid collisions
        let original_name = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        let trash_name = generate_unique_name(&trash_subdir, original_name);
        let trash_path = trash_subdir.join(&trash_name);

        // Get file size before moving
        let size_bytes = std::fs::metadata(file_path)
            .map(|m| m.len() as i64)
            .ok();

        // Move file to trash
        std::fs::rename(file_path, &trash_path).map_err(|e| {
            AppError::io(format!(
                "Cannot move file to trash: {} -> {}: {}",
                file_path.display(),
                trash_path.display(),
                e
            ))
        })?;

        // Record in database
        let auto_delete_at = (Utc::now() + Duration::days(TRASH_RETENTION_DAYS))
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();

        let entry = LocalTrashEntry {
            id: None,
            original_path: file_path.to_string_lossy().to_string(),
            trash_path: trash_path.to_string_lossy().to_string(),
            sync_zone: sync_zone.to_string(),
            size_bytes,
            deleted_at: None, // DB default
            auto_delete_at,
        };

        db::add_to_local_trash(&self.db, &entry)?;

        log::info!(
            "Moved to trash: {} -> {}",
            file_path.display(),
            trash_path.display()
        );

        Ok(trash_path)
    }

    /// Restore a file from trash to its original location.
    pub fn restore_file(&self, trash_entry_id: i64) -> AppResult<PathBuf> {
        let entries = db::list_local_trash(&self.db)?;
        let entry = entries
            .iter()
            .find(|e| e.id == Some(trash_entry_id))
            .ok_or_else(|| AppError::io("Trash entry not found"))?;

        let trash_path = Path::new(&entry.trash_path);
        let original_path = Path::new(&entry.original_path);

        if !trash_path.exists() {
            db::remove_from_local_trash(&self.db, trash_entry_id)?;
            return Err(AppError::io("File no longer exists in trash"));
        }

        // Create parent directory if needed
        if let Some(parent) = original_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                AppError::io(format!("Cannot create directory: {}", e))
            })?;
        }

        // Move back to original location
        std::fs::rename(trash_path, original_path).map_err(|e| {
            AppError::io(format!(
                "Cannot restore file: {} -> {}: {}",
                trash_path.display(),
                original_path.display(),
                e
            ))
        })?;

        // Remove from database
        db::remove_from_local_trash(&self.db, trash_entry_id)?;

        log::info!(
            "Restored from trash: {} -> {}",
            trash_path.display(),
            original_path.display()
        );

        Ok(original_path.to_path_buf())
    }

    /// List all files in local trash.
    pub fn list(&self) -> AppResult<Vec<LocalTrashEntry>> {
        db::list_local_trash(&self.db)
    }

    /// Remove all files from local trash permanently.
    pub fn clear(&self) -> AppResult<usize> {
        let entries = db::list_local_trash(&self.db)?;
        let mut removed = 0;

        for entry in &entries {
            let trash_path = Path::new(&entry.trash_path);
            if trash_path.exists() {
                if let Err(e) = std::fs::remove_file(trash_path) {
                    log::warn!("Cannot delete trash file {}: {}", trash_path.display(), e);
                }
            }
            if let Some(id) = entry.id {
                db::remove_from_local_trash(&self.db, id)?;
            }
            removed += 1;
        }

        // Clean up empty date directories
        self.cleanup_empty_dirs();

        log::info!("Cleared local trash: {} files removed", removed);
        Ok(removed)
    }

    /// Remove expired trash entries (older than TRASH_RETENTION_DAYS).
    pub fn cleanup_expired(&self) -> AppResult<usize> {
        let expired_count = db::cleanup_expired_trash(&self.db)?;

        if expired_count > 0 {
            // Also remove orphaned files in trash directory
            self.cleanup_empty_dirs();
            log::info!("Cleaned up {} expired trash entries", expired_count);
        }

        Ok(expired_count)
    }

    /// Remove empty date subdirectories from trash.
    fn cleanup_empty_dirs(&self) {
        if let Ok(entries) = std::fs::read_dir(&self.trash_dir) {
            for entry in entries.flatten() {
                if let Ok(ft) = entry.file_type() {
                    if ft.is_dir() {
                        // Remove if empty
                        if std::fs::read_dir(entry.path())
                            .map(|mut d| d.next().is_none())
                            .unwrap_or(false)
                        {
                            let _ = std::fs::remove_dir(entry.path());
                        }
                    }
                }
            }
        }
    }
}

/// Generate a unique filename in the target directory.
/// If "report.pdf" exists, tries "report (1).pdf", "report (2).pdf", etc.
fn generate_unique_name(dir: &Path, name: &str) -> String {
    let target = dir.join(name);
    if !target.exists() {
        return name.to_string();
    }

    let stem = Path::new(name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(name);
    let ext = Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{}", e))
        .unwrap_or_default();

    for i in 1..1000 {
        let candidate = format!("{} ({}){}", stem, i, ext);
        if !dir.join(&candidate).exists() {
            return candidate;
        }
    }

    // Fallback: use timestamp
    format!("{}_{}{}", stem, Utc::now().timestamp(), ext)
}
