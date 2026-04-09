use crate::config::{ActivityEntry, AppConfig, SyncStatus};
use crate::error::{AppError, AppResult};
use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;

/// Payload emitted to the frontend via the `sync-progress` event.
#[derive(Clone, serde::Serialize)]
pub struct SyncProgressPayload {
    pub phase: String,
    pub file_count: usize,
    pub message: String,
    pub source: String,
}

/// Count file operations from rclone verbose output.
fn count_sync_operations(stdout: &str, stderr: &str) -> usize {
    // First try: parse the "Transferred:  N / N" line (file count, not bytes)
    for line in stderr.lines().chain(stdout.lines()).rev() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("Transferred:") {
            let rest = rest.trim();
            // Skip the bytes line (contains "B" like "1.234 GiB")
            if rest.contains('B') {
                continue;
            }
            // Parse "5 / 10, 50%" — get first number
            if let Some(count_str) = rest.split('/').next() {
                if let Ok(count) = count_str.trim().parse::<usize>() {
                    return count;
                }
            }
        }
    }

    // Fallback: count operation lines from bisync verbose output
    stdout
        .lines()
        .chain(stderr.lines())
        .filter(|line| {
            line.contains(": Copied")
                || line.contains(": Deleted")
                || line.contains(": Moved")
                || line.contains("copy-file")
                || line.contains("delete-file")
                || line.contains("move-file")
        })
        .count()
}

/// Sync engine that wraps rclone bisync for bidirectional synchronization.
pub struct SyncEngine {
    status: Mutex<SyncStatus>,
    activity_log: Mutex<Vec<ActivityEntry>>,
}

impl SyncEngine {
    pub fn new() -> Self {
        Self {
            status: Mutex::new(SyncStatus::NotConfigured),
            activity_log: Mutex::new(Vec::new()),
        }
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

        // Ensure local directories exist (async to avoid blocking the runtime)
        tokio::fs::create_dir_all(&config.personal_sync_path)
            .await
            .map_err(|e| self.set_error_status(format!("Cannot create personal dir: {}", e)))?;
        tokio::fs::create_dir_all(&config.shared_sync_path)
            .await
            .map_err(|e| self.set_error_status(format!("Cannot create shared dir: {}", e)))?;

        // Obscure the token for rclone
        let obscured_token = self
            .obscure_password(app, token)
            .await
            .map_err(|e| self.set_error_status(e.to_string()))?;

        // Sync personal files
        let personal_result = self
            .run_bisync(
                app,
                &config.personal_webdav_url(),
                &config.personal_sync_path.to_string_lossy(),
                &config.user_login,
                &obscured_token,
            )
            .await;

        if let Err(ref e) = personal_result {
            self.log_activity("sync_personal", "", "error", Some(e.to_string()));
        } else {
            self.log_activity("sync_personal", "", "success", None);
        }

        // Sync shared files
        let shared_result = self
            .run_bisync(
                app,
                &config.shared_webdav_url(),
                &config.shared_sync_path.to_string_lossy(),
                &config.user_login,
                &obscured_token,
            )
            .await;

        if let Err(ref e) = shared_result {
            self.log_activity("sync_shared", "", "error", Some(e.to_string()));
        } else {
            self.log_activity("sync_shared", "", "success", None);
        }

        // Update status based on results
        match (&personal_result, &shared_result) {
            (Ok(personal_count), Ok(shared_count)) => {
                self.set_status(SyncStatus::Idle);
                let total = personal_count + shared_count;
                let _ = app.emit(
                    "sync-progress",
                    SyncProgressPayload {
                        phase: "completed".to_string(),
                        file_count: total,
                        message: if total > 0 {
                            format!("Zsynchronizowano {} plików", total)
                        } else {
                            "Wszystko aktualne".to_string()
                        },
                        source: source.to_string(),
                    },
                );
                Ok(())
            }
            _ => {
                let error = format!(
                    "{}{}",
                    personal_result
                        .as_ref()
                        .err()
                        .map(|e| format!("Personal: {}. ", e))
                        .unwrap_or_default(),
                    shared_result
                        .as_ref()
                        .err()
                        .map(|e| format!("Shared: {}", e))
                        .unwrap_or_default()
                )
                .trim()
                .to_string();

                self.set_status(SyncStatus::Error(error.clone()));
                let _ = app.emit(
                    "sync-progress",
                    SyncProgressPayload {
                        phase: "error".to_string(),
                        file_count: 0,
                        message: error.clone(),
                        source: source.to_string(),
                    },
                );
                Err(AppError::sync(error))
            }
        }
    }

    /// Run rclone bisync between a WebDAV remote and a local directory.
    /// Auth credentials are passed via environment variables (not visible in /proc/pid/cmdline).
    /// Returns the number of files transferred on success.
    async fn run_bisync(
        &self,
        app: &AppHandle,
        webdav_url: &str,
        local_path: &str,
        username: &str,
        obscured_token: &str,
    ) -> AppResult<usize> {
        // Clean up any legacy in-directory marker from older versions —
        // rclone would try to upload it and nginx blocks dotfiles.
        let legacy_marker = Path::new(local_path).join(".veloryn-sync-init");
        if legacy_marker.exists() {
            let _ = std::fs::remove_file(&legacy_marker);
        }

        // Check if this is the first sync run. Marker lives in the app data
        // directory so rclone never tries to sync it.
        let marker_dir = dirs::data_local_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("com.veloryn.cloudfile")
            .join("markers");
        let _ = std::fs::create_dir_all(&marker_dir);
        let marker_name = format!(
            "sync-init-{}",
            local_path
                .replace(['/', '\\', ':', ' '], "_")
                .trim_matches('_')
                .to_string()
        );
        let first_run_marker = marker_dir.join(marker_name);
        let is_first_run = !first_run_marker.exists();

        let mut args = vec![
            "bisync",
            ":webdav:",
            local_path,
            "--create-empty-src-dirs",
            "--resilient",
            "--conflict-resolve=newer",
            "--verbose",
            // nginx on the Veloryn backend blocks all dotfiles (/\.(?!well-known)),
            // so skip them entirely to avoid 403 errors during sync.
            "--exclude",
            ".*",
            "--exclude",
            ".*/**",
        ];

        if is_first_run {
            args.push("--resync");
        } else {
            args.push("--recover");
        }

        log::info!("Running rclone bisync for {}", webdav_url);

        let output = tokio::time::timeout(
            Duration::from_secs(1800),
            app.shell()
                .sidecar("sidecars/rclone")
                .map_err(|e| AppError::sync(format!("Failed to create rclone sidecar: {}", e)))?
                .args(&args)
                .env("RCLONE_WEBDAV_URL", webdav_url)
                .env("RCLONE_WEBDAV_USER", username)
                .env("RCLONE_WEBDAV_PASS", obscured_token)
                .output(),
        )
        .await
        .map_err(|_| AppError::sync("Synchronizacja przekroczyła limit czasu"))?
        .map_err(|e| AppError::sync(format!("Failed to run rclone: {}", e)))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        log::debug!("rclone stdout: {}", stdout);
        if !stderr.is_empty() {
            log::warn!("rclone stderr: {}", stderr);
        }

        if output.status.success() {
            // Mark first sync complete
            if is_first_run {
                let _ = std::fs::write(&first_run_marker, "initialized");
            }
            let file_count = count_sync_operations(&stdout, &stderr);
            log::info!("rclone bisync completed: {} file(s) transferred", file_count);
            Ok(file_count)
        } else {
            let error = if stderr.is_empty() {
                format!("rclone exited with code {:?}", output.status.code())
            } else {
                stderr.to_string()
            };

            // Check for conflicts
            if error.contains("CONFLICT") || error.contains("conflict") {
                self.set_status(SyncStatus::Conflict);
            }

            Err(AppError::sync(error))
        }
    }

    /// Obscure a password for rclone (rclone uses its own obscure format).
    async fn obscure_password(&self, app: &AppHandle, password: &str) -> AppResult<String> {
        let (mut rx, mut child) = app
            .shell()
            .sidecar("sidecars/rclone")
            .map_err(|e| AppError::sync(format!("Failed to create rclone sidecar: {}", e)))?
            .args(["obscure", "-"])
            .spawn()
            .map_err(|e| AppError::sync(format!("Failed to spawn rclone: {}", e)))?;

        child
            .write(format!("{}\n", password).as_bytes())
            .map_err(|e| {
                AppError::sync(format!("Failed to write password to rclone stdin: {}", e))
            })?;

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut exit_code = None;

        while let Some(event) = rx.recv().await {
            match event {
                CommandEvent::Stdout(line) => stdout.extend(line),
                CommandEvent::Stderr(line) => stderr.extend(line),
                CommandEvent::Error(err) => {
                    return Err(AppError::sync(format!(
                        "Failed to obscure password: {}",
                        err
                    )));
                }
                CommandEvent::Terminated(payload) => {
                    exit_code = payload.code;
                    break;
                }
                _ => {}
            }
        }

        if exit_code == Some(0) {
            Ok(String::from_utf8_lossy(&stdout).trim().to_string())
        } else {
            let stderr = String::from_utf8_lossy(&stderr).trim().to_string();
            if stderr.is_empty() {
                Err(AppError::sync(format!(
                    "Failed to obscure password: rclone exited with code {:?}",
                    exit_code
                )))
            } else {
                Err(AppError::sync(format!(
                    "Failed to obscure password: {}",
                    stderr
                )))
            }
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
