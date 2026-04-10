use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

/// Application configuration stored in the Tauri store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Server URL (e.g., "https://docs.company.com")
    pub server_url: String,

    /// User login (used as WebDAV username)
    pub user_login: String,

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

    /// Max upload speed in KB/s (0 = unlimited)
    #[serde(default)]
    pub max_upload_kbps: u64,

    /// Max download speed in KB/s (0 = unlimited)
    #[serde(default)]
    pub max_download_kbps: u64,

    /// Folders to include in sync (empty = sync all)
    #[serde(default)]
    pub sync_include_paths: Vec<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let base = home.join("CloudFile");

        Self {
            server_url: String::new(),
            user_login: String::new(),
            tenant_id: String::new(),
            personal_sync_path: base.join("Moje pliki"),
            shared_sync_path: base.join("Udostępnione"),
            sync_interval_secs: 300,
            watch_local_changes: true,
            sync_on_startup: true,
            max_file_size_bytes: 0,
            max_upload_kbps: 0,
            max_download_kbps: 0,
            sync_include_paths: Vec::new(),
        }
    }
}

impl AppConfig {
    /// Check if the configuration is complete (user logged in)
    pub fn is_configured(&self) -> bool {
        !self.server_url.is_empty() && !self.user_login.is_empty()
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
    /// Sync paused by user
    Paused,
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

const STORE_FILE: &str = "config.json";
const STORE_KEY: &str = "app_config";

/// Load config from tauri-plugin-store (persisted across restarts).
pub fn load_config(app: &AppHandle) -> Option<AppConfig> {
    let store = app.store(STORE_FILE).ok()?;
    let value = store.get(STORE_KEY)?;
    serde_json::from_value(value).ok()
}

/// Save config to tauri-plugin-store.
pub fn save_config(app: &AppHandle, config: &AppConfig) -> AppResult<()> {
    let store = app
        .store(STORE_FILE)
        .map_err(|e| AppError::config(format!("Nie udało się otworzyć store: {}", e)))?;
    let value = serde_json::to_value(config).map_err(|e| {
        AppError::config(format!("Nie udało się zserializować konfiguracji: {}", e))
    })?;
    store.set(STORE_KEY.to_string(), value);
    store
        .save()
        .map_err(|e| AppError::config(format!("Nie udało się zapisać konfiguracji: {}", e)))
}

/// Clear config from tauri-plugin-store (e.g. on logout).
pub fn clear_config(app: &AppHandle) -> AppResult<()> {
    let store = app
        .store(STORE_FILE)
        .map_err(|e| AppError::config(format!("Nie udało się otworzyć store: {}", e)))?;
    let _ = store.delete(STORE_KEY);
    store
        .save()
        .map_err(|e| AppError::config(format!("Nie udało się wyczyścić konfiguracji: {}", e)))
}
