use crate::config::{ActivityEntry, AppConfig, SyncStatus};
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};

/// Sync engine that wraps rclone bisync for bidirectional synchronization.
pub struct SyncEngine {
    pub status: Arc<Mutex<SyncStatus>>,
    pub activity_log: Arc<Mutex<Vec<ActivityEntry>>>,
    rclone_path: String,
}

impl SyncEngine {
    pub fn new(rclone_path: String) -> Self {
        Self {
            status: Arc::new(Mutex::new(SyncStatus::NotConfigured)),
            activity_log: Arc::new(Mutex::new(Vec::new())),
            rclone_path,
        }
    }

    /// Run a full bidirectional sync for both personal and shared files.
    pub fn sync_all(&self, config: &AppConfig, token: &str) -> Result<(), String> {
        if !config.is_configured() {
            return Err("Not configured".to_string());
        }

        *self.status.lock().unwrap() = SyncStatus::Syncing;

        // Ensure local directories exist
        std::fs::create_dir_all(&config.personal_sync_path)
            .map_err(|e| format!("Cannot create personal dir: {}", e))?;
        std::fs::create_dir_all(&config.shared_sync_path)
            .map_err(|e| format!("Cannot create shared dir: {}", e))?;

        // Sync personal files
        let personal_result = self.run_bisync(
            &config.personal_webdav_url(),
            &config.personal_sync_path.to_string_lossy(),
            &config.user_email,
            token,
        );

        if let Err(ref e) = personal_result {
            self.log_activity("sync_personal", "", "error", Some(e.clone()));
        } else {
            self.log_activity("sync_personal", "", "success", None);
        }

        // Sync shared files
        let shared_result = self.run_bisync(
            &config.shared_webdav_url(),
            &config.shared_sync_path.to_string_lossy(),
            &config.user_email,
            token,
        );

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
    fn run_bisync(
        &self,
        webdav_url: &str,
        local_path: &str,
        username: &str,
        token: &str,
    ) -> Result<(), String> {
        // First, ensure resync is done (required for first bisync)
        let first_run_marker = Path::new(local_path).join(".readynextos-sync-init");
        let is_first_run = !first_run_marker.exists();

        let mut args = vec![
            "bisync".to_string(),
            format!(":webdav:{{}}", ""),
            local_path.to_string(),
            format!("--webdav-url={}", webdav_url),
            format!("--webdav-user={}", username),
            format!("--webdav-pass={}", self.obscure_password(token)?),
            "--create-empty-src-dirs".to_string(),
            "--resilient".to_string(),
            "--conflict-resolve=newer".to_string(),
            "--verbose".to_string(),
        ];

        if is_first_run {
            args.push("--resync".to_string());
        } else {
            args.push("--recover".to_string());
        }

        log::info!("Running rclone bisync for {}", webdav_url);

        let output = Command::new(&self.rclone_path)
            .args(&args)
            .output()
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
    fn obscure_password(&self, password: &str) -> Result<String, String> {
        let output = Command::new(&self.rclone_path)
            .args(["obscure", password])
            .output()
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
            log.drain(0..log.len() - 1000);
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
