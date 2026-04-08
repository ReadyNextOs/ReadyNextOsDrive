// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod auth;
mod config;
mod error;
mod sync;
mod watcher;

use config::{ActivityEntry, AppConfig, SyncStatus};
use error::{AppError, AppResult};
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use std::sync::{Arc, Mutex, MutexGuard};
use tauri::image::Image;
use tauri::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State};

/// Shared application state
struct AppState {
    config: Mutex<AppConfig>,
    sync_engine: Arc<sync::SyncEngine>,
    watcher: Mutex<watcher::FileWatcher>,
    scheduler: Mutex<SchedulerState>,
}

#[derive(Default)]
struct SchedulerState {
    startup_sync_done: bool,
    last_sync_attempt: Option<Instant>,
    last_watch_sync_attempt: Option<Instant>,
}

impl AppState {
    fn config(&self) -> MutexGuard<'_, AppConfig> {
        self.config.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn watcher(&self) -> MutexGuard<'_, watcher::FileWatcher> {
        self.watcher.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn scheduler(&self) -> MutexGuard<'_, SchedulerState> {
        self.scheduler
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn validate_server_url(server_url: &str) -> AppResult<()> {
    let parsed = reqwest::Url::parse(server_url.trim())
        .map_err(|_| AppError::config("Nieprawidłowy adres serwera"))?;

    if parsed.scheme() != "https" {
        return Err(AppError::config("Adres serwera musi używać HTTPS"));
    }

    let host = parsed
        .host_str()
        .ok_or_else(|| AppError::config("Adres serwera musi zawierać host"))?;

    if matches!(host, "localhost" | "127.0.0.1" | "::1") {
        return Err(AppError::config("Adresy lokalne nie są dozwolone"));
    }

    if let Ok(ip) = host.parse::<IpAddr>() {
        let blocked = match ip {
            IpAddr::V4(ip) => {
                ip.is_loopback()
                    || ip.is_private()
                    || ip.is_link_local()
                    || ip.is_unspecified()
                    || ip.is_multicast()
            }
            IpAddr::V6(ip) => {
                ip.is_loopback()
                    || ip.is_unspecified()
                    || ip.is_multicast()
                    || ip.is_unique_local()
            }
        };

        if blocked {
            return Err(AppError::config("Adresy lokalne i prywatne nie są dozwolone"));
        }
    }

    Ok(())
}

fn validate_sync_path(path: &Path) -> AppResult<()> {
    if path.as_os_str().is_empty() {
        return Err(AppError::config("Ścieżka synchronizacji nie może być pusta"));
    }

    if !path.is_absolute() {
        return Err(AppError::config("Ścieżka synchronizacji musi być bezwzględna"));
    }

    if path == Path::new("/") {
        return Err(AppError::config("Nie można użyć katalogu głównego jako ścieżki synchronizacji"));
    }

    Ok(())
}

fn validate_config(config: &AppConfig) -> AppResult<()> {
    validate_server_url(&config.server_url)?;

    if config.user_email.trim().is_empty() {
        return Err(AppError::config("Brak adresu e-mail użytkownika"));
    }

    if config.sync_interval_secs < 30 || config.sync_interval_secs > 3600 {
        return Err(AppError::config("Interwał synchronizacji musi być w zakresie 30-3600 sekund"));
    }

    validate_sync_path(&config.personal_sync_path)?;
    validate_sync_path(&config.shared_sync_path)?;

    if config.personal_sync_path == config.shared_sync_path {
        return Err(AppError::config("Ścieżki synchronizacji muszą być różne"));
    }

    Ok(())
}

fn normalize_existing_path(path: &Path) -> AppResult<PathBuf> {
    std::fs::canonicalize(path)
        .map_err(|e| AppError::io(format!("Nieprawidłowa ścieżka: {}", e)))
}

fn path_is_within(path: &Path, allowed_roots: &[PathBuf]) -> bool {
    allowed_roots.iter().any(|root| path.starts_with(root))
}

fn record_sync_attempt(state: &AppState, watch_triggered: bool) {
    let now = Instant::now();
    let mut scheduler = state.scheduler();
    scheduler.startup_sync_done = true;
    scheduler.last_sync_attempt = Some(now);
    if watch_triggered {
        scheduler.last_watch_sync_attempt = Some(now);
    }
}

fn reset_scheduler_for_login_or_config(state: &AppState) {
    let now = Instant::now();
    let mut scheduler = state.scheduler();
    scheduler.startup_sync_done = false;
    scheduler.last_sync_attempt = Some(now);
    scheduler.last_watch_sync_attempt = Some(now);
}

fn reset_scheduler_for_logout(state: &AppState) {
    let mut scheduler = state.scheduler();
    scheduler.startup_sync_done = false;
    scheduler.last_sync_attempt = None;
    scheduler.last_watch_sync_attempt = None;
}

fn configure_watcher_for_current_config(state: &AppState) -> AppResult<()> {
    let cfg = state.config().clone();
    let mut watcher = state.watcher();
    watcher.stop();

    if !cfg.is_configured() || !cfg.watch_local_changes {
        return Ok(());
    }

    std::fs::create_dir_all(&cfg.personal_sync_path)
        .map_err(|e| AppError::io(format!("Nie udało się utworzyć katalogu osobistego: {}", e)))?;
    std::fs::create_dir_all(&cfg.shared_sync_path)
        .map_err(|e| AppError::io(format!("Nie udało się utworzyć katalogu współdzielonego: {}", e)))?;

    watcher
        .start(&[cfg.personal_sync_path.as_path(), cfg.shared_sync_path.as_path()])
        .map_err(AppError::sync)
}

async fn run_sync_once(app: &tauri::AppHandle, state: &AppState, watch_triggered: bool) -> AppResult<()> {
    let cfg = state.config().clone();
    if !cfg.is_configured() {
        return Err(AppError::sync("Aplikacja nie jest skonfigurowana"));
    }

    let token = auth::get_token(&cfg.user_email)?
        .ok_or_else(|| AppError::auth("Brak tokenu logowania"))?;

    record_sync_attempt(state, watch_triggered);
    state.sync_engine.sync_all(app, &cfg, &token.token).await
}

async fn run_scheduler_tick(app: &tauri::AppHandle) {
    let state = app.state::<AppState>();
    let cfg = state.config().clone();
    if !cfg.is_configured() {
        return;
    }

    let now = Instant::now();
    let (should_startup_sync, should_interval_sync, should_watch_sync) = {
        let scheduler = state.scheduler();
        let startup = cfg.sync_on_startup && !scheduler.startup_sync_done;
        let interval_due = scheduler
            .last_sync_attempt
            .map(|last| now.duration_since(last) >= Duration::from_secs(cfg.sync_interval_secs))
            .unwrap_or(false);
        let watch_interval_due = scheduler
            .last_watch_sync_attempt
            .map(|last| now.duration_since(last) >= Duration::from_secs(5))
            .unwrap_or(true);
        drop(scheduler);
        let watch_has_changes = {
            let watcher = state.watcher();
            watcher.is_running() && watcher.has_changes()
        };
        let watch_due = cfg.watch_local_changes
            && watch_has_changes
            && watch_interval_due;
        (startup, interval_due, watch_due)
    };

    let should_sync = should_startup_sync || should_interval_sync || should_watch_sync;
    if !should_sync {
        return;
    }

    let result = run_sync_once(app, &state, should_watch_sync).await;

    let mut scheduler = state.scheduler();
    if should_startup_sync {
        scheduler.startup_sync_done = true;
    }

    if let Err(err) = result {
        log::warn!("Background sync failed: {}", err);
    }
}

// ==================== Tauri Commands ====================

/// Login with email and password
#[tauri::command]
async fn login(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    server_url: String,
    email: String,
    password: String,
) -> Result<String, String> {
    validate_server_url(&server_url).map_err(|e| e.to_string())?;

    let response = auth::login(&server_url, &email, &password)
        .await
        .map_err(|e| e.to_string())?;
    persist_login(&app, &state, server_url, email, response.token, response.user)
        .map_err(|e| e.to_string())
}

/// Login with a short-lived desktop bootstrap token.
#[tauri::command]
async fn login_with_token(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    token: String,
) -> Result<String, String> {
    let response = auth::exchange_desktop_token(&token)
        .await
        .map_err(|e| e.to_string())?;
    validate_server_url(&response.config.server_url).map_err(|e| e.to_string())?;
    persist_login(
        &app,
        &state,
        response.config.server_url,
        response.user.email.clone(),
        response.token,
        response.user,
    )
    .map_err(|e| e.to_string())
}

fn persist_login(
    app: &tauri::AppHandle,
    state: &State<'_, AppState>,
    server_url: String,
    email: String,
    api_token: String,
    user: auth::LoginUser,
) -> AppResult<String> {
    // Store token in keychain
    let token = auth::AuthToken {
        token: api_token,
        token_type: auth::TokenType::Sanctum,
        expires_at: None,
    };
    auth::store_token(&email, &token)?;

    let user_json = serde_json::to_string(&user)
        .map_err(|e| AppError::internal(format!("Błąd serializacji użytkownika: {}", e)))?;

    // Update config and persist
    {
        let mut cfg = state.config();
        cfg.server_url = server_url;
        cfg.user_email = email;
        cfg.tenant_id = user.tenant_id;
        config::save_config(app, &cfg)?;
    }

    reset_scheduler_for_login_or_config(state);
    configure_watcher_for_current_config(state)?;

    Ok(user_json)
}

/// Logout and remove stored credentials
#[tauri::command]
fn logout(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let email = state.config().user_email.clone();
    if !email.is_empty() {
        auth::remove_token(&email).map_err(|e| e.to_string())?;
    }

    state.watcher().stop();
    *state.config() = AppConfig::default();
    state.sync_engine.set_not_configured();
    reset_scheduler_for_logout(&state);

    // Clear persisted config
    config::clear_config(&app).map_err(|e| e.to_string())?;

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
    state.config().clone()
}

/// Update configuration
#[tauri::command]
fn update_config(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    config: AppConfig,
) -> Result<(), String> {
    validate_config(&config).map_err(|e| e.to_string())?;
    config::save_config(&app, &config).map_err(|e| e.to_string())?;
    *state.config() = config;
    reset_scheduler_for_login_or_config(&state);
    configure_watcher_for_current_config(&state).map_err(|e| e.to_string())?;
    Ok(())
}

/// Trigger manual sync
#[tauri::command]
async fn trigger_sync(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    run_sync_once(&app, &state, false)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Get recent activity log
#[tauri::command]
fn get_activity(state: State<'_, AppState>, limit: Option<usize>) -> Vec<ActivityEntry> {
    state.sync_engine.get_activity(limit.unwrap_or(50))
}

/// Open a local folder in the file manager
#[tauri::command]
fn open_folder(state: State<'_, AppState>, path: String) -> Result<(), String> {
    let requested = normalize_existing_path(Path::new(&path)).map_err(|e| e.to_string())?;
    if !requested.is_dir() {
        return Err("Można otwierać tylko istniejące katalogi".to_string());
    }

    let cfg = state.config().clone();
    let allowed_roots = [
        normalize_existing_path(&cfg.personal_sync_path),
        normalize_existing_path(&cfg.shared_sync_path),
    ]
    .into_iter()
    .collect::<AppResult<Vec<_>>>()
    .map_err(|e| e.to_string())?;

    if !path_is_within(&requested, &allowed_roots) {
        return Err("Ścieżka poza katalogami synchronizacji".to_string());
    }

    open::that(&requested).map_err(|e| format!("Failed to open folder: {}", e))
}

/// Pick a folder using native OS dialog
#[tauri::command]
async fn pick_folder(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let folder = app
        .dialog()
        .file()
        .set_title("Wybierz folder synchronizacji")
        .blocking_pick_folder();
    Ok(folder.map(|f| f.to_string()))
}

/// Update the tray icon based on current sync status
fn update_tray_icon(app: &AppHandle, status: &SyncStatus) {
    let icon_bytes: &[u8] = match status {
        SyncStatus::Idle => include_bytes!("../icons/tray-idle.png"),
        SyncStatus::Syncing => include_bytes!("../icons/tray-syncing.png"),
        SyncStatus::Error(_) => include_bytes!("../icons/tray-error.png"),
        SyncStatus::Conflict => include_bytes!("../icons/tray-error.png"),
        SyncStatus::NotConfigured => include_bytes!("../icons/tray-icon.png"),
    };

    let tooltip = match status {
        SyncStatus::Idle => "Veloryn CloudFile — zsynchronizowano",
        SyncStatus::Syncing => "Veloryn CloudFile — synchronizacja...",
        SyncStatus::Error(_) => "Veloryn CloudFile — błąd",
        SyncStatus::Conflict => "Veloryn CloudFile — konflikt",
        SyncStatus::NotConfigured => "Veloryn CloudFile",
    };

    if let Some(tray) = app.tray_by_id("main-tray") {
        if let Ok(image) = Image::from_bytes(icon_bytes) {
            let _ = tray.set_icon(Some(image));
        }
        let _ = tray.set_tooltip(Some(tooltip));
    }
}

fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn build_tray_menu(app: &AppHandle, status: &SyncStatus) -> tauri::Result<Menu<tauri::Wry>> {
    let status_label = match status {
        SyncStatus::Idle => "● Zsynchronizowano",
        SyncStatus::Syncing => "◌ Synchronizacja...",
        SyncStatus::Error(_) => "✕ Błąd synchronizacji",
        SyncStatus::Conflict => "⚠ Konflikt",
        SyncStatus::NotConfigured => "○ Nie skonfigurowano",
    };

    let status_item = MenuItem::with_id(app, "status", status_label, false, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let open_personal = MenuItem::with_id(app, "open_personal", "Otwórz Moje pliki", true, None::<&str>)?;
    let open_shared = MenuItem::with_id(app, "open_shared", "Otwórz Udostępnione", true, None::<&str>)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let sync_now = MenuItem::with_id(app, "sync_now", "Synchronizuj teraz", !matches!(status, SyncStatus::Syncing), None::<&str>)?;
    let sep3 = PredefinedMenuItem::separator(app)?;
    let settings_item = MenuItem::with_id(app, "settings", "Ustawienia...", true, None::<&str>)?;
    let sep4 = PredefinedMenuItem::separator(app)?;
    let quit_item = MenuItem::with_id(app, "quit", "Zakończ", true, None::<&str>)?;

    Menu::with_items(app, &[
        &status_item, &sep1,
        &open_personal, &open_shared, &sep2,
        &sync_now, &sep3,
        &settings_item, &sep4,
        &quit_item,
    ])
}

fn handle_tray_menu_event(app: &AppHandle, event: MenuEvent) {
    match event.id.as_ref() {
        "open_personal" => {
            let state = app.state::<AppState>();
            let path = state.config().personal_sync_path.clone();
            let _ = open::that(&path);
        }
        "open_shared" => {
            let state = app.state::<AppState>();
            let path = state.config().shared_sync_path.clone();
            let _ = open::that(&path);
        }
        "sync_now" => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let state = app.state::<AppState>();
                let cfg = state.config().clone();
                let email = cfg.user_email.clone();
                if let Ok(Some(token)) = auth::get_token(&email) {
                    let engine = state.sync_engine.clone();
                    // Update tray icon to syncing
                    update_tray_icon(&app, &SyncStatus::Syncing);
                    let result_status = match engine.sync_all(&app, &cfg, &token.token).await {
                        Ok(()) => SyncStatus::Idle,
                        Err(_) => engine.get_status(),
                    };
                    update_tray_icon(&app, &result_status);
                    // Rebuild tray menu with new status
                    if let Some(tray) = app.tray_by_id("main-tray") {
                        if let Ok(menu) = build_tray_menu(&app, &result_status) {
                            let _ = tray.set_menu(Some(menu));
                        }
                    }
                }
            });
        }
        "settings" => {
            show_window(app);
            // Frontend will handle showing settings tab via event
            let _ = app.emit("navigate", "settings");
        }
        "show" => {
            show_window(app);
        }
        "quit" => {
            app.exit(0);
        }
        _ => {}
    }
}

// ==================== Main ====================

fn main() {
    // Workaround for WebKitGTK EGL crashes on Linux (Intel Iris Xe + Wayland)
    // Step 1: COMPOSITING_MODE=1 prevents EGL init crash during webview creation
    // Step 2: In setup(), we set HardwareAccelerationPolicy::Never and reload
    #[cfg(target_os = "linux")]
    {
        if std::env::var("WEBKIT_DISABLE_COMPOSITING_MODE").is_err() {
            std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
        }
        if std::env::var("WEBKIT_DISABLE_DMABUF_RENDERER").is_err() {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
    }

    env_logger::init();

    let sync_engine = Arc::new(sync::SyncEngine::new());

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            config: Mutex::new(AppConfig::default()),
            sync_engine: sync_engine.clone(),
            watcher: Mutex::new(watcher::FileWatcher::new()),
            scheduler: Mutex::new(SchedulerState::default()),
        })
        .invoke_handler(tauri::generate_handler![
            login,
            login_with_token,
            logout,
            get_sync_status,
            get_config,
            update_config,
            trigger_sync,
            get_activity,
            open_folder,
            pick_folder,
        ])
        .setup(|app| {
            // Load persisted config from store
            if let Some(saved_config) = config::load_config(app.handle()) {
                let state = app.state::<AppState>();
                *state.config() = saved_config;
                if let Err(err) = configure_watcher_for_current_config(&state) {
                    log::warn!("Failed to configure watcher on startup: {}", err);
                }
            }

            {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let mut interval = tokio::time::interval(Duration::from_secs(5));
                    loop {
                        interval.tick().await;
                        run_scheduler_tick(&handle).await;
                    }
                });
            }

            // On Linux: disable hardware acceleration and reload to fix EGL blank page
            #[cfg(target_os = "linux")]
            {
                if let Some(window) = app.get_webview_window("main") {
                    window.with_webview(|webview| {
                        use webkit2gtk::WebViewExt;
                        use webkit2gtk::SettingsExt;
                        if let Some(settings) = WebViewExt::settings(&webview.inner()) {
                            SettingsExt::set_hardware_acceleration_policy(
                                &settings,
                                webkit2gtk::HardwareAccelerationPolicy::Never,
                            );
                        }
                    }).ok();

                    // Remove COMPOSITING_MODE so new subprocesses render normally
                    std::env::remove_var("WEBKIT_DISABLE_COMPOSITING_MODE");

                    // Reload page — new subprocess uses software rendering (no EGL)
                    let _ = window.eval("setTimeout(() => window.location.reload(), 100)");
                }
            }

            // Setup tray icon with expanded menu
            let initial_status = app.state::<AppState>().sync_engine.get_status();
            update_tray_icon(app.handle(), &initial_status);

            let menu = build_tray_menu(app.handle(), &initial_status)?;

            if let Some(tray) = app.tray_by_id("main-tray") {
                tray.set_menu(Some(menu))?;
                tray.set_show_menu_on_left_click(false)?;

                // Left click: show/focus window
                tray.on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_window(tray.app_handle());
                    }
                });

                // Menu events
                tray.on_menu_event(|app, event| {
                    handle_tray_menu_event(app, event);
                });
            }

            // Hide main window on startup (tray-only mode)
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.hide();
            }

            log::info!("Veloryn CloudFile started");
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
        .expect("error while running Veloryn CloudFile");
}
