// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod auth;
mod config;
mod sync;
mod watcher;

use config::{ActivityEntry, AppConfig, SyncStatus};
use std::sync::{Arc, Mutex};
use tauri::{Manager, State};

/// Shared application state
struct AppState {
    config: Mutex<AppConfig>,
    sync_engine: Arc<sync::SyncEngine>,
    watcher: Mutex<watcher::FileWatcher>,
}

// ==================== Tauri Commands ====================

/// Login with email and password
#[tauri::command]
async fn login(
    state: State<'_, AppState>,
    server_url: String,
    email: String,
    password: String,
) -> Result<String, String> {
    let response = auth::login(&server_url, &email, &password).await?;

    // Store token in keychain
    let token = auth::AuthToken {
        token: response.token.clone(),
        token_type: auth::TokenType::Sanctum,
        expires_at: None,
    };
    auth::store_token(&email, &token)?;

    // Update config
    {
        let mut config = state.config.lock().unwrap();
        config.server_url = server_url;
        config.user_email = email;
        config.tenant_id = response.user.tenant_id;
    }

    Ok(serde_json::to_string(&response.user).unwrap())
}

/// Logout and remove stored credentials
#[tauri::command]
fn logout(state: State<'_, AppState>) -> Result<(), String> {
    let email = state.config.lock().unwrap().user_email.clone();
    if !email.is_empty() {
        auth::remove_token(&email)?;
    }

    // Stop watcher
    state.watcher.lock().unwrap().stop();

    // Reset config
    *state.config.lock().unwrap() = AppConfig::default();
    *state.sync_engine.status.lock().unwrap() = SyncStatus::NotConfigured;

    Ok(())
}

/// Get current sync status
#[tauri::command]
fn get_sync_status(state: State<'_, AppState>) -> SyncStatus {
    state.sync_engine.get_status()
}

/// Get configuration
#[tauri::command]
fn get_config(state: State<'_, AppState>) -> AppConfig {
    state.config.lock().unwrap().clone()
}

/// Update configuration
#[tauri::command]
fn update_config(state: State<'_, AppState>, config: AppConfig) -> Result<(), String> {
    *state.config.lock().unwrap() = config;
    Ok(())
}

/// Trigger manual sync
#[tauri::command]
async fn trigger_sync(state: State<'_, AppState>) -> Result<(), String> {
    let config = state.config.lock().unwrap().clone();
    let email = config.user_email.clone();

    let token = auth::get_token(&email)?
        .ok_or_else(|| "Not logged in".to_string())?;

    state.sync_engine.sync_all(&config, &token.token)?;

    Ok(())
}

/// Get recent activity log
#[tauri::command]
fn get_activity(state: State<'_, AppState>, limit: Option<usize>) -> Vec<ActivityEntry> {
    state.sync_engine.get_activity(limit.unwrap_or(50))
}

/// Open a local folder in the file manager
#[tauri::command]
fn open_folder(path: String) -> Result<(), String> {
    open::that(&path).map_err(|e| format!("Failed to open folder: {}", e))
}

// ==================== Main ====================

fn main() {
    env_logger::init();

    // Determine rclone sidecar path
    let rclone_path = "rclone".to_string(); // Will be resolved as sidecar

    let sync_engine = Arc::new(sync::SyncEngine::new(rclone_path));

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(AppState {
            config: Mutex::new(AppConfig::default()),
            sync_engine: sync_engine.clone(),
            watcher: Mutex::new(watcher::FileWatcher::new()),
        })
        .invoke_handler(tauri::generate_handler![
            login,
            logout,
            get_sync_status,
            get_config,
            update_config,
            trigger_sync,
            get_activity,
            open_folder,
        ])
        .setup(|app| {
            // Hide main window on startup (tray-only mode)
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.hide();
            }

            log::info!("ReadyNextOs Drive started");
            Ok(())
        })
        .on_window_event(|window, event| {
            // Hide window instead of closing (keep in tray)
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running ReadyNextOs Drive");
}
