use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Application configuration stored in the Tauri store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Server URL (e.g., "https://docs.company.com")
    pub server_url: String,

    /// User email (used as WebDAV username)
    pub user_email: String,

    /// Tenant ID
    pub tenant_id: String,

    /// Local sync directory for personal files
    pub personal_sync_path: PathBuf,

    /// Local sync directory for shared files
    pub shared_sync_path: PathBuf,

    /// Sync interval in seconds (default: 300 = 5 minutes)
    pub sync_interval_secs: u64,

    /// Whether to watch for local file changes
    pub watch_local_changes: bool,

    /// Whether to sync on startup
    pub sync_on_startup: bool,

    /// Maximum file size to sync (bytes, 0 = unlimited)
    pub max_file_size_bytes: u64,
}

impl Default for AppConfig {
    fn default() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let base = home.join("ReadyNextOs");

        Self {
            server_url: String::new(),
            user_email: String::new(),
            tenant_id: String::new(),
            personal_sync_path: base.join("Moje pliki"),
            shared_sync_path: base.join("Udostępnione"),
            sync_interval_secs: 300,
            watch_local_changes: true,
            sync_on_startup: true,
            max_file_size_bytes: 0,
        }
    }
}

impl AppConfig {
    /// Check if the configuration is complete (user logged in)
    pub fn is_configured(&self) -> bool {
        !self.server_url.is_empty() && !self.user_email.is_empty()
    }

    /// Get the WebDAV URL for personal files
    pub fn personal_webdav_url(&self) -> String {
        format!("{}/dav/personal", self.server_url.trim_end_matches('/'))
    }

    /// Get the WebDAV URL for shared files
    pub fn shared_webdav_url(&self) -> String {
        format!("{}/dav/shared", self.server_url.trim_end_matches('/'))
    }
}

/// Sync status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SyncStatus {
    /// Everything is synced
    Idle,
    /// Sync in progress
    Syncing,
    /// There's a conflict to resolve
    Conflict,
    /// Connection error
    Error(String),
    /// Not configured / not logged in
    NotConfigured,
}

/// Activity log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityEntry {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub action: String,
    pub file_path: String,
    pub status: String,
    pub details: Option<String>,
}
