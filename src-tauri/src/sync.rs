use crate::config::{ActivityEntry, AppConfig, SyncStatus};
use std::path::Path;
use std::sync::{Arc, Mutex};
use tauri::AppHandle;
use tauri_plugin_shell::ShellExt;

/// Sync engine that wraps rclone bisync for bidirectional synchronization.
pub struct SyncEngine {
    pub status: Arc<Mutex<SyncStatus>>,
    pub activity_log: Arc<Mutex<Vec<ActivityEntry>>>,
}

impl SyncEngine {
    pub fn new() -> Self {
        Self {
            status: Arc::new(Mutex::new(SyncStatus::NotConfigured)),
            activity_log: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Run a full bidirectional sync for both personal and shared files.
    pub async fn sync_all(
        &self,
        app: &AppHandle,
        config: &AppConfig,
        token: &str,
    ) -> Result<(), String> {
        if !config.is_configured() {
            return Err("Not configured".to_string());
        }

        *self.status.lock().unwrap() = SyncStatus::Syncing;

        // Ensure local directories exist
        std::fs::create_dir_all(&config.personal_sync_path)
            .map_err(|e| format!("Cannot create personal dir: {}", e))?;
        std::fs::create_dir_all(&config.shared_sync_path)
            .map_err(|e| format!("Cannot create shared dir: {}", e))?;

        // Obscure the token for rclone
        let obscured_token = self.obscure_password(app, token).await?;

        // Sync personal files
        let personal_result = self
            .run_bisync(
                app,
                &config.personal_webdav_url(),
                &config.personal_sync_path.to_string_lossy(),
                &config.user_email,
                &obscured_token,
            )
            .await;

        if let Err(ref e) = personal_result {
            self.log_activity("sync_personal", "", "error", Some(e.clone()));
        } else {
            self.log_activity("sync_personal", "", "success", None);
        }

        // Sync shared files
        let shared_result = self
            .run_bisync(
                app,
                &config.shared_webdav_url(),
                &config.shared_sync_path.to_string_lossy(),
                &config.user_email,
                &obscured_token,
            )
            .await;

        if let Err(ref e) = shared_result {
            self.log_activity("sync_shared", "", "error", Some(e.clone()));
        } else {
            self.log_activity("sync_shared", "", "success", None);
        }

        // Update status based on results
        match (&personal_result, &shared_result) {
            (Ok(()), Ok(())) => {
                *self.status.lock().unwrap() = SyncStatus::Idle;
            }
            _ => {
                let error = personal_result
                    .err()
                    .or(shared_result.err())
                    .unwrap_or_default();
                *self.status.lock().unwrap() = SyncStatus::Error(error);
            }
        }

        Ok(())
    }

    /// Run rclone bisync between a WebDAV remote and a local directory.
    /// Auth credentials are passed via environment variables (not visible in /proc/pid/cmdline).
    async fn run_bisync(
        &self,
        app: &AppHandle,
        webdav_url: &str,
        local_path: &str,
        username: &str,
        obscured_token: &str,
    ) -> Result<(), String> {
        // Check if this is the first sync run
        let first_run_marker = Path::new(local_path).join(".readynextos-sync-init");
        let is_first_run = !first_run_marker.exists();

        let mut args = vec![
            "bisync",
            ":webdav:",
            local_path,
            "--create-empty-src-dirs",
            "--resilient",
            "--conflict-resolve=newer",
            "--verbose",
        ];

        if is_first_run {
            args.push("--resync");
        } else {
            args.push("--recover");
        }

        log::info!("Running rclone bisync for {}", webdav_url);

        let output = app
            .shell()
            .sidecar("sidecars/rclone")
            .map_err(|e| format!("Failed to create rclone sidecar: {}", e))?
            .args(&args)
            .env("RCLONE_WEBDAV_URL", webdav_url)
            .env("RCLONE_WEBDAV_USER", username)
            .env("RCLONE_WEBDAV_PASS", obscured_token)
            .output()
            .await
            .map_err(|e| format!("Failed to run rclone: {}", e))?;

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
            Ok(())
        } else {
            let error = if stderr.is_empty() {
                format!("rclone exited with code {:?}", output.status.code())
            } else {
                stderr.to_string()
            };

            // Check for conflicts
            if error.contains("CONFLICT") || error.contains("conflict") {
                *self.status.lock().unwrap() = SyncStatus::Conflict;
            }

            Err(error)
        }
    }

    /// Obscure a password for rclone (rclone uses its own obscure format).
    async fn obscure_password(&self, app: &AppHandle, password: &str) -> Result<String, String> {
        let output = app
            .shell()
            .sidecar("sidecars/rclone")
            .map_err(|e| format!("Failed to create rclone sidecar: {}", e))?
            .args(["obscure", password])
            .output()
            .await
            .map_err(|e| format!("Failed to obscure password: {}", e))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            Err("Failed to obscure password".to_string())
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

        let mut log = self.activity_log.lock().unwrap();
        log.push(entry);

        // Keep only last 1000 entries
        if log.len() > 1000 {
            let excess = log.len() - 1000;
            log.drain(0..excess);
        }
    }

    /// Get the current sync status.
    pub fn get_status(&self) -> SyncStatus {
        self.status.lock().unwrap().clone()
    }

    /// Get recent activity entries.
    pub fn get_activity(&self, limit: usize) -> Vec<ActivityEntry> {
        let log = self.activity_log.lock().unwrap();
        let start = if log.len() > limit {
            log.len() - limit
        } else {
            0
        };
        log[start..].to_vec()
    }
}
