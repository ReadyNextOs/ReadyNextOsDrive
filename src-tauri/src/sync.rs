use crate::config::{ActivityEntry, AppConfig, SyncStatus};
use crate::db::{self, DbPool, FileState, SyncRunStats};
use crate::diff::{compute_diff, LocalFileInfo, RemoteFileInfo, SyncAction};
use crate::error::{AppError, AppResult};
use crate::transfer::WebDavTransfer;
use crate::trash::LocalTrashManager;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use tauri::{AppHandle, Emitter};

/// Payload emitted to the frontend via the `sync-progress` event.
#[derive(Clone, serde::Serialize)]
pub struct SyncProgressPayload {
    pub phase: String,
    pub file_count: usize,
    pub message: String,
    pub source: String,
}

/// Per-file progress event emitted during sync.
#[derive(Clone, serde::Serialize)]
struct SyncFileProgressPayload {
    path: String,
    action: String,
    current: usize,
    total: usize,
}

/// Sync engine that performs native bidirectional sync via WebDAV + SQLite state.
pub struct SyncEngine {
    status: Mutex<SyncStatus>,
    activity_log: Mutex<Vec<ActivityEntry>>,
    /// Lazily initialized DB pool (None until first sync).
    db: Mutex<Option<DbPool>>,
}

impl SyncEngine {
    pub fn new() -> Self {
        Self {
            status: Mutex::new(SyncStatus::NotConfigured),
            activity_log: Mutex::new(Vec::new()),
            db: Mutex::new(None),
        }
    }

    /// Get or initialize the SQLite DB pool (lazy, created on first sync).
    fn get_db(&self) -> AppResult<DbPool> {
        let mut guard = self
            .db
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if guard.is_none() {
            *guard = Some(db::init_db()?);
        }
        Ok(guard.as_ref().expect("just initialized").clone())
    }

    /// Run a full bidirectional sync for both personal and shared files.
    pub async fn sync_all(
        &self,
        app: &AppHandle,
        config: &AppConfig,
        token: &str,
        source: &str,
    ) -> AppResult<()> {
        if !config.is_configured() {
            return Err(AppError::sync("Aplikacja nie jest skonfigurowana"));
        }

        {
            let mut status = self.status_guard();
            if *status == SyncStatus::Syncing {
                return Err(AppError::sync("Synchronizacja już w toku"));
            }
            *status = SyncStatus::Syncing;
        }

        // Notify frontend that sync has started
        let _ = app.emit(
            "sync-progress",
            SyncProgressPayload {
                phase: "started".to_string(),
                file_count: 0,
                message: "Synchronizacja...".to_string(),
                source: source.to_string(),
            },
        );

        // Ensure local directories exist
        tokio::fs::create_dir_all(&config.personal_sync_path)
            .await
            .map_err(|e| self.set_error_status(format!("Cannot create personal dir: {}", e)))?;
        tokio::fs::create_dir_all(&config.shared_sync_path)
            .await
            .map_err(|e| self.set_error_status(format!("Cannot create shared dir: {}", e)))?;

        // Initialize DB (once, cached)
        let db = self
            .get_db()
            .map_err(|e| self.set_error_status(e.to_string()))?;

        // Start sync run tracking
        let run_id = db::start_sync_run(&db, source)
            .map_err(|e| self.set_error_status(e.to_string()))?;

        let mut total_stats = SyncRunStats {
            files_uploaded: 0,
            files_downloaded: 0,
            files_deleted: 0,
            files_conflicted: 0,
            bytes_transferred: 0,
            error_message: None,
        };

        // Sync personal zone
        let personal_result = self
            .sync_zone(
                app,
                &db,
                "personal",
                &config.personal_sync_path,
                &config.personal_webdav_url(),
                token,
                source,
            )
            .await;

        let personal_ok = match &personal_result {
            Ok(stats) => {
                total_stats.files_uploaded += stats.files_uploaded;
                total_stats.files_downloaded += stats.files_downloaded;
                total_stats.files_deleted += stats.files_deleted;
                total_stats.files_conflicted += stats.files_conflicted;
                total_stats.bytes_transferred += stats.bytes_transferred;
                self.log_activity("sync_personal", "", "success", None);
                true
            }
            Err(e) => {
                self.log_activity("sync_personal", "", "error", Some(e.to_string()));
                false
            }
        };

        // Sync shared zone
        let shared_result = self
            .sync_zone(
                app,
                &db,
                "shared",
                &config.shared_sync_path,
                &config.shared_webdav_url(),
                token,
                source,
            )
            .await;

        let shared_ok = match &shared_result {
            Ok(stats) => {
                total_stats.files_uploaded += stats.files_uploaded;
                total_stats.files_downloaded += stats.files_downloaded;
                total_stats.files_deleted += stats.files_deleted;
                total_stats.files_conflicted += stats.files_conflicted;
                total_stats.bytes_transferred += stats.bytes_transferred;
                self.log_activity("sync_shared", "", "success", None);
                true
            }
            Err(e) => {
                self.log_activity("sync_shared", "", "error", Some(e.to_string()));
                false
            }
        };

        // Cleanup expired local trash
        let trash_dir = default_trash_dir();
        let trash = LocalTrashManager::new(trash_dir, db.clone());
        let _ = trash.cleanup_expired();

        // Determine final status
        let total_files = (total_stats.files_uploaded
            + total_stats.files_downloaded
            + total_stats.files_deleted) as usize;

        let (run_status, final_status, emit_phase, emit_message) =
            if personal_ok && shared_ok {
                let message = if total_files > 0 {
                    format!("Zsynchronizowano {} plików", total_files)
                } else {
                    "Wszystko aktualne".to_string()
                };
                ("success", SyncStatus::Idle, "completed".to_string(), message)
            } else if personal_ok || shared_ok {
                let err = collect_errors(&personal_result, &shared_result);
                total_stats.error_message = Some(err);
                (
                    "partial",
                    SyncStatus::Idle,
                    "completed".to_string(),
                    "Synchronizacja częściowa".to_string(),
                )
            } else {
                let err = collect_errors(&personal_result, &shared_result);
                total_stats.error_message = Some(err.clone());
                (
                    "error",
                    SyncStatus::Error(err.clone()),
                    "error".to_string(),
                    err,
                )
            };

        let _ = db::complete_sync_run(&db, run_id, run_status, &total_stats);

        self.set_status(final_status);

        let _ = app.emit(
            "sync-progress",
            SyncProgressPayload {
                phase: emit_phase.clone(),
                file_count: total_files,
                message: emit_message.clone(),
                source: source.to_string(),
            },
        );

        if emit_phase == "error" {
            Err(AppError::sync(emit_message))
        } else {
            Ok(())
        }
    }

    /// Sync a single zone (personal or shared).
    async fn sync_zone(
        &self,
        app: &AppHandle,
        db: &DbPool,
        zone: &str,
        local_base: &Path,
        webdav_url: &str,
        token: &str,
        _source: &str,
    ) -> AppResult<SyncRunStats> {
        log::info!("Starting sync zone '{}' local={}", zone, local_base.display());

        let transfer = WebDavTransfer::new(webdav_url, token);

        // 1. Scan local files
        let local_files = scan_local_files(local_base)?;
        log::debug!("Zone '{}': {} local files found", zone, local_files.len());

        // 2. Scan remote files
        let remote_files = scan_remote_files(&transfer).await?;
        log::debug!("Zone '{}': {} remote files found", zone, remote_files.len());

        // 3. Load known states from SQLite
        let known_states = db::list_files_by_zone(db, zone)?;
        log::debug!("Zone '{}': {} known states in DB", zone, known_states.len());

        // 4. Run 3-way diff (local and remote paths are relative to their bases)
        let local_base_str = local_base.to_string_lossy().to_string();
        let actions = compute_diff(
            &local_files,
            &remote_files,
            &known_states,
            zone,
            &local_base_str,
            "", // remote paths are relative; WebDavTransfer prepends the base URL
        );

        log::info!("Zone '{}': {} sync actions to perform", zone, actions.len());

        let total = actions.len();
        let mut stats = SyncRunStats {
            files_uploaded: 0,
            files_downloaded: 0,
            files_deleted: 0,
            files_conflicted: 0,
            bytes_transferred: 0,
            error_message: None,
        };

        // Set up trash manager
        let trash = LocalTrashManager::new(default_trash_dir(), db.clone());

        // 5. Execute each action
        for (idx, diff_result) in actions.iter().enumerate() {
            let current = idx + 1;

            let action_str = action_label(&diff_result.action);
            let _ = app.emit(
                "sync-file-progress",
                SyncFileProgressPayload {
                    path: diff_result.path.clone(),
                    action: action_str.to_string(),
                    current,
                    total,
                },
            );

            let exec_result = self
                .execute_action(
                    app,
                    db,
                    &transfer,
                    &trash,
                    zone,
                    local_base,
                    &diff_result.action,
                    &diff_result.path,
                )
                .await;

            match exec_result {
                Ok(bytes) => match &diff_result.action {
                    SyncAction::Upload { .. } => {
                        stats.files_uploaded += 1;
                        stats.bytes_transferred += bytes as i64;
                    }
                    SyncAction::Download { .. } => {
                        stats.files_downloaded += 1;
                        stats.bytes_transferred += bytes as i64;
                    }
                    SyncAction::DeleteLocal { .. } | SyncAction::DeleteRemote { .. } => {
                        stats.files_deleted += 1;
                    }
                    SyncAction::Conflict { .. } => {
                        stats.files_conflicted += 1;
                    }
                    SyncAction::Skip => {}
                },
                Err(e) => {
                    log::warn!("Zone '{}': error on '{}': {}", zone, diff_result.path, e);
                    let _ = record_file_error(db, &diff_result.path, zone, &e.to_string());
                    self.log_activity(
                        &format!("sync_{}", zone),
                        &diff_result.path,
                        "error",
                        Some(e.to_string()),
                    );
                }
            }
        }

        Ok(stats)
    }

    /// Execute a single sync action. Returns bytes transferred (0 for deletes/conflicts).
    async fn execute_action(
        &self,
        app: &AppHandle,
        db: &DbPool,
        transfer: &WebDavTransfer,
        trash: &LocalTrashManager,
        zone: &str,
        _local_base: &Path,
        action: &SyncAction,
        rel_path: &str,
    ) -> AppResult<u64> {
        match action {
            SyncAction::Upload {
                local_path,
                remote_path,
            } => {
                let local = Path::new(local_path);
                let remote = remote_path.trim_start_matches('/');

                log::info!("Upload: {} -> {}", local_path, remote);
                let result = transfer.upload_file(local, remote).await?;

                let new_etag = fetch_etag(transfer, remote).await.unwrap_or_default();

                let metadata = std::fs::metadata(local)
                    .map_err(|e| AppError::io(format!("Cannot stat {}: {}", local_path, e)))?;
                let hash = hash_file(local)?;
                let mtime = mtime_from_metadata(&metadata);

                let state = FileState {
                    path: rel_path.to_string(),
                    sync_zone: zone.to_string(),
                    local_hash: Some(hash.clone()),
                    local_mtime: Some(mtime),
                    local_size: Some(metadata.len() as i64),
                    local_exists: true,
                    remote_etag: non_empty_str(&new_etag),
                    remote_mtime: None,
                    remote_size: Some(result.bytes_sent as i64),
                    remote_exists: true,
                    sync_status: "synced".to_string(),
                    last_synced_hash: Some(hash),
                    last_synced_etag: non_empty_str(&new_etag),
                    last_synced_at: Some(chrono::Utc::now().to_rfc3339()),
                    error_message: None,
                    retry_count: 0,
                };
                db::upsert_file_state(db, &state)?;

                self.log_activity("upload", rel_path, "success", None);
                Ok(result.bytes_sent)
            }

            SyncAction::Download {
                remote_path,
                local_path,
            } => {
                let local = Path::new(local_path);
                let remote = remote_path.trim_start_matches('/');

                // Ensure parent directory exists
                if let Some(parent) = local.parent() {
                    tokio::fs::create_dir_all(parent)
                        .await
                        .map_err(|e| AppError::io(format!("Cannot create dir: {}", e)))?;
                }

                log::info!("Download: {} -> {}", remote, local_path);
                let result = transfer.download_file(remote, local).await?;

                let metadata = std::fs::metadata(local)
                    .map_err(|e| AppError::io(format!("Cannot stat {}: {}", local_path, e)))?;
                let hash = hash_file(local)?;
                let mtime = mtime_from_metadata(&metadata);

                let new_etag = fetch_etag(transfer, remote).await.unwrap_or_default();

                let state = FileState {
                    path: rel_path.to_string(),
                    sync_zone: zone.to_string(),
                    local_hash: Some(hash.clone()),
                    local_mtime: Some(mtime),
                    local_size: Some(metadata.len() as i64),
                    local_exists: true,
                    remote_etag: non_empty_str(&new_etag),
                    remote_mtime: None,
                    remote_size: Some(result.bytes_received as i64),
                    remote_exists: true,
                    sync_status: "synced".to_string(),
                    last_synced_hash: Some(hash),
                    last_synced_etag: non_empty_str(&new_etag),
                    last_synced_at: Some(chrono::Utc::now().to_rfc3339()),
                    error_message: None,
                    retry_count: 0,
                };
                db::upsert_file_state(db, &state)?;

                self.log_activity("download", rel_path, "success", None);
                Ok(result.bytes_received)
            }

            SyncAction::DeleteLocal { local_path } => {
                let local = Path::new(local_path);
                if local.exists() {
                    log::info!("DeleteLocal (trash): {}", local_path);
                    trash.trash_file(local, zone)?;
                }
                let _ = db::delete_file_state(db, rel_path, zone);
                self.log_activity("delete_local", rel_path, "success", None);
                Ok(0)
            }

            SyncAction::DeleteRemote { remote_path } => {
                let remote = remote_path.trim_start_matches('/');
                log::info!("DeleteRemote: {}", remote);
                transfer.delete_remote(remote).await?;
                let _ = db::delete_file_state(db, rel_path, zone);
                self.log_activity("delete_remote", rel_path, "success", None);
                Ok(0)
            }

            SyncAction::Conflict {
                local_path,
                remote_path,
                conflict_type,
            } => {
                log::warn!(
                    "Conflict '{}' ({:?}): auto-skipping (Phase 4 resolution pending)",
                    rel_path,
                    conflict_type
                );

                let _ = app.emit(
                    "sync-conflict",
                    serde_json::json!({
                        "path": rel_path,
                        "localPath": local_path,
                        "remotePath": remote_path,
                        "conflictType": conflict_type,
                        "resolution": "skipped",
                    }),
                );

                // Record conflict state in DB
                let existing = db::get_file_state(db, rel_path, zone)?;
                let base = existing.unwrap_or(FileState {
                    path: rel_path.to_string(),
                    sync_zone: zone.to_string(),
                    local_hash: None,
                    local_mtime: None,
                    local_size: None,
                    local_exists: true,
                    remote_etag: None,
                    remote_mtime: None,
                    remote_size: None,
                    remote_exists: true,
                    sync_status: "conflict".to_string(),
                    last_synced_hash: None,
                    last_synced_etag: None,
                    last_synced_at: None,
                    error_message: None,
                    retry_count: 0,
                });
                let updated = FileState {
                    sync_status: "conflict".to_string(),
                    error_message: Some(format!("Conflict: {:?}", conflict_type)),
                    ..base
                };
                db::upsert_file_state(db, &updated)?;

                self.log_activity(
                    "conflict",
                    rel_path,
                    "skipped",
                    Some(format!("{:?}", conflict_type)),
                );
                Ok(0)
            }

            SyncAction::Skip => Ok(0),
        }
    }

    fn log_activity(&self, action: &str, file_path: &str, status: &str, details: Option<String>) {
        let entry = ActivityEntry {
            timestamp: chrono::Utc::now(),
            action: action.to_string(),
            file_path: file_path.to_string(),
            status: status.to_string(),
            details,
        };

        let mut log = self.activity_log_guard();
        log.push(entry);

        // Keep only last 1000 entries
        if log.len() > 1000 {
            let excess = log.len() - 1000;
            log.drain(0..excess);
        }
    }

    /// Get the current sync status.
    pub fn get_status(&self) -> SyncStatus {
        self.status_guard().clone()
    }

    /// Get recent activity entries.
    pub fn get_activity(&self, limit: usize) -> Vec<ActivityEntry> {
        let log = self.activity_log_guard();
        let start = if log.len() > limit {
            log.len() - limit
        } else {
            0
        };
        log[start..].to_vec()
    }

    pub fn set_not_configured(&self) {
        self.set_status(SyncStatus::NotConfigured);
    }

    pub fn set_idle(&self) {
        self.set_status(SyncStatus::Idle);
    }

    fn set_error_status(&self, error: String) -> AppError {
        self.set_status(SyncStatus::Error(error.clone()));
        AppError::sync(error)
    }

    fn set_status(&self, status: SyncStatus) {
        *self.status_guard() = status;
    }

    fn status_guard(&self) -> MutexGuard<'_, SyncStatus> {
        self.status
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn activity_log_guard(&self) -> MutexGuard<'_, Vec<ActivityEntry>> {
        self.activity_log
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

// ==================== Local File Scanning ====================

const MAX_FILE_SIZE: u64 = 500 * 1024 * 1024; // 500 MB

/// Recursively scan local directory, hashing files with blake3.
/// Skips dotfiles and files > 500MB.
fn scan_local_files(base: &Path) -> AppResult<Vec<LocalFileInfo>> {
    let mut files = Vec::new();

    if !base.exists() {
        return Ok(files);
    }

    for entry in walkdir::WalkDir::new(base)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();

        // Skip directories
        if path.is_dir() {
            continue;
        }

        // Skip dotfiles (any path component starting with '.')
        if has_dotfile_component(path, base) {
            continue;
        }

        let metadata = match std::fs::metadata(path) {
            Ok(m) => m,
            Err(e) => {
                log::warn!("Cannot stat {}: {}", path.display(), e);
                continue;
            }
        };

        // Skip files > 500MB
        if metadata.len() > MAX_FILE_SIZE {
            log::warn!("Skipping large file (>500MB): {}", path.display());
            continue;
        }

        // Compute relative path
        let rel_path = match path.strip_prefix(base) {
            Ok(p) => p.to_string_lossy().replace('\\', "/"),
            Err(_) => continue,
        };

        if rel_path.is_empty() {
            continue;
        }

        let hash = match hash_file(path) {
            Ok(h) => h,
            Err(e) => {
                log::warn!("Cannot hash {}: {}", path.display(), e);
                continue;
            }
        };

        let mtime = mtime_from_metadata(&metadata);

        files.push(LocalFileInfo {
            path: rel_path,
            hash,
            mtime,
            size: metadata.len() as i64,
        });
    }

    Ok(files)
}

/// Returns true if any component of `path` (relative to `base`) starts with '.'.
fn has_dotfile_component(path: &Path, base: &Path) -> bool {
    if let Ok(rel) = path.strip_prefix(base) {
        for component in rel.components() {
            if let std::path::Component::Normal(name) = component {
                if name.to_string_lossy().starts_with('.') {
                    return true;
                }
            }
        }
    }
    false
}

/// Compute blake3 hash of a file, returned as hex string.
fn hash_file(path: &Path) -> AppResult<String> {
    let bytes = std::fs::read(path).map_err(|e| {
        AppError::io(format!(
            "Cannot read file for hashing {}: {}",
            path.display(),
            e
        ))
    })?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

/// Extract modification time as unix timestamp from file metadata.
fn mtime_from_metadata(metadata: &std::fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Convert an empty string to None.
fn non_empty_str(s: &str) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

// ==================== Remote File Scanning ====================

/// Recursively scan remote WebDAV directory via PROPFIND.
/// Returns flat list of RemoteFileInfo for files only.
async fn scan_remote_files(transfer: &WebDavTransfer) -> AppResult<Vec<RemoteFileInfo>> {
    let mut stack: Vec<String> = vec!["".to_string()];
    let mut files = Vec::new();

    while let Some(dir) = stack.pop() {
        let entries = match transfer.propfind(&dir).await {
            Ok(e) => e,
            Err(e) => {
                log::warn!("PROPFIND failed for '{}': {}", dir, e);
                continue;
            }
        };

        for entry in entries {
            if entry.is_directory {
                // Push subdirectory onto stack for iteration
                let subdir = decode_webdav_href(&entry.path);
                stack.push(subdir);
            } else {
                let rel_path = relative_remote_path(&entry.path);
                if rel_path.is_empty() {
                    continue;
                }
                // Skip dotfiles
                if rel_path.split('/').any(|c| c.starts_with('.')) {
                    continue;
                }
                files.push(RemoteFileInfo {
                    path: rel_path,
                    etag: entry.etag,
                    mtime: entry
                        .last_modified
                        .as_deref()
                        .and_then(parse_http_date),
                    size: entry.size.map(|s| s as i64),
                });
            }
        }
    }

    Ok(files)
}

/// Decode a WebDAV href to a plain path string (no URL encoding).
fn decode_webdav_href(href: &str) -> String {
    urlencoding::decode(href)
        .map(|c| c.into_owned())
        .unwrap_or_else(|_| href.to_string())
        .trim_end_matches('/')
        .to_string()
}

/// Convert a full WebDAV href (e.g. "/dav/personal/folder/file.txt") to a
/// relative path suitable for the 3-way diff (e.g. "folder/file.txt").
fn relative_remote_path(href: &str) -> String {
    let decoded = urlencoding::decode(href)
        .map(|c| c.into_owned())
        .unwrap_or_else(|_| href.to_string());

    let normalized = decoded.trim_start_matches('/');

    // Strip known prefix segments: "dav/personal/", "dav/shared/", etc.
    for prefix in &[
        "dav/personal/",
        "dav/shared/",
        "personal/",
        "shared/",
        "backend/dav/personal/",
        "backend/dav/shared/",
    ] {
        if let Some(rest) = find_after(normalized, prefix) {
            let rel = rest.trim_end_matches('/');
            return rel.to_string();
        }
    }

    // Fallback: strip first two path segments (zone prefix)
    let parts: Vec<&str> = normalized.split('/').filter(|s| !s.is_empty()).collect();
    if parts.len() > 2 {
        parts[2..].join("/")
    } else if parts.len() == 2 {
        parts[1].to_string()
    } else {
        normalized.trim_end_matches('/').to_string()
    }
}

/// Find the substring of `s` after the first occurrence of `needle`.
fn find_after<'a>(s: &'a str, needle: &str) -> Option<&'a str> {
    s.find(needle).map(|pos| &s[pos + needle.len()..])
}

/// Parse an HTTP date string ("Mon, 01 Jan 2024 00:00:00 GMT") to unix timestamp.
fn parse_http_date(s: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc2822(s)
        .ok()
        .map(|dt| dt.timestamp())
}

/// Fetch the etag for a remote file by PROPFIND on its parent directory.
async fn fetch_etag(transfer: &WebDavTransfer, remote_path: &str) -> AppResult<String> {
    let parent = Path::new(remote_path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    let entries = transfer.propfind(&parent).await.unwrap_or_default();

    let file_name = Path::new(remote_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    for entry in entries {
        if entry.name == file_name {
            return Ok(entry.etag.unwrap_or_default());
        }
    }

    Ok(String::new())
}

// ==================== DB Helpers ====================

/// Record a per-file error in the database, incrementing retry_count.
fn record_file_error(db: &DbPool, path: &str, zone: &str, error: &str) -> AppResult<()> {
    let existing = db::get_file_state(db, path, zone)?.unwrap_or(FileState {
        path: path.to_string(),
        sync_zone: zone.to_string(),
        local_hash: None,
        local_mtime: None,
        local_size: None,
        local_exists: false,
        remote_etag: None,
        remote_mtime: None,
        remote_size: None,
        remote_exists: false,
        sync_status: "error".to_string(),
        last_synced_hash: None,
        last_synced_etag: None,
        last_synced_at: None,
        error_message: None,
        retry_count: 0,
    });

    let updated = FileState {
        sync_status: "error".to_string(),
        error_message: Some(error.to_string()),
        retry_count: existing.retry_count + 1,
        ..existing
    };

    db::upsert_file_state(db, &updated)
}

// ==================== Misc Helpers ====================

/// Default trash directory for this application.
fn default_trash_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.veloryn.cloudfile")
        .join("trash")
}

/// Build a combined error message from two zone results.
fn collect_errors(
    personal: &AppResult<SyncRunStats>,
    shared: &AppResult<SyncRunStats>,
) -> String {
    let mut parts = Vec::new();
    if let Err(e) = personal {
        parts.push(format!("Personal: {}", e));
    }
    if let Err(e) = shared {
        parts.push(format!("Shared: {}", e));
    }
    parts.join(". ")
}

/// Short label for a sync action (used in progress events).
fn action_label(action: &SyncAction) -> &'static str {
    match action {
        SyncAction::Upload { .. } => "upload",
        SyncAction::Download { .. } => "download",
        SyncAction::DeleteLocal { .. } => "delete_local",
        SyncAction::DeleteRemote { .. } => "delete_remote",
        SyncAction::Conflict { .. } => "conflict",
        SyncAction::Skip => "skip",
    }
}
