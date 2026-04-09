// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod auth;
mod config;
mod db;
mod diff;
mod error;
mod sync;
mod transfer;
mod trash;
mod watcher;

use config::{ActivityEntry, AppConfig, SyncStatus};
use error::{AppError, AppResult};
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};
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
    debug_enabled: AtomicBool,
    log_path: PathBuf,
}

#[derive(Default)]
struct SchedulerState {
    startup_sync_done: bool,
    last_sync_attempt: Option<Instant>,
    last_watch_sync_attempt: Option<Instant>,
}

impl AppState {
    fn config(&self) -> MutexGuard<'_, AppConfig> {
        self.config
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn watcher(&self) -> MutexGuard<'_, watcher::FileWatcher> {
        self.watcher
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
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
                ip.is_loopback() || ip.is_unspecified() || ip.is_multicast() || ip.is_unique_local()
            }
        };

        if blocked {
            return Err(AppError::config(
                "Adresy lokalne i prywatne nie są dozwolone",
            ));
        }
    }

    Ok(())
}

fn validate_sync_path(path: &Path) -> AppResult<()> {
    if path.as_os_str().is_empty() {
        return Err(AppError::config(
            "Ścieżka synchronizacji nie może być pusta",
        ));
    }

    if !path.is_absolute() {
        return Err(AppError::config(
            "Ścieżka synchronizacji musi być bezwzględna",
        ));
    }

    if path == Path::new("/") {
        return Err(AppError::config(
            "Nie można użyć katalogu głównego jako ścieżki synchronizacji",
        ));
    }

    // Block Windows drive roots (C:\, D:\, etc.)
    #[cfg(target_os = "windows")]
    if path.parent().is_none() || path.as_os_str().len() <= 3 {
        return Err(AppError::config(
            "Nie można użyć katalogu głównego dysku jako ścieżki synchronizacji",
        ));
    }

    Ok(())
}

fn validate_config(config: &AppConfig) -> AppResult<()> {
    validate_server_url(&config.server_url)?;

    if config.user_login.trim().is_empty() {
        return Err(AppError::config("Brak loginu użytkownika"));
    }

    if config.sync_interval_secs < 30 || config.sync_interval_secs > 3600 {
        return Err(AppError::config(
            "Interwał synchronizacji musi być w zakresie 30-3600 sekund",
        ));
    }

    validate_sync_path(&config.personal_sync_path)?;
    validate_sync_path(&config.shared_sync_path)?;

    if config.personal_sync_path == config.shared_sync_path {
        return Err(AppError::config("Ścieżki synchronizacji muszą być różne"));
    }

    Ok(())
}

fn normalize_existing_path(path: &Path) -> AppResult<PathBuf> {
    std::fs::canonicalize(path).map_err(|e| AppError::io(format!("Nieprawidłowa ścieżka: {}", e)))
}

fn path_is_within(path: &Path, allowed_roots: &[PathBuf]) -> bool {
    allowed_roots.iter().any(|root| path.starts_with(root))
}

fn record_sync_attempt(state: &AppState, source: &str) {
    let now = Instant::now();
    let mut scheduler = state.scheduler();
    scheduler.startup_sync_done = true;
    scheduler.last_sync_attempt = Some(now);
    if source == "watcher" {
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
    std::fs::create_dir_all(&cfg.shared_sync_path).map_err(|e| {
        AppError::io(format!(
            "Nie udało się utworzyć katalogu współdzielonego: {}",
            e
        ))
    })?;

    watcher
        .start(&[
            cfg.personal_sync_path.as_path(),
            cfg.shared_sync_path.as_path(),
        ])
        .map_err(AppError::sync)
}

async fn run_sync_once(
    app: &tauri::AppHandle,
    state: &AppState,
    source: &str,
) -> AppResult<()> {
    let cfg = state.config().clone();
    log::info!(
        "run_sync_once: login={}, server={}, configured={}, source={}",
        cfg.user_login,
        cfg.server_url,
        cfg.is_configured(),
        source
    );
    if !cfg.is_configured() {
        return Err(AppError::sync("Aplikacja nie jest skonfigurowana"));
    }

    let token = auth::get_token(&cfg.user_login)?.ok_or_else(|| {
        log::error!("run_sync_once: no token in keychain for {}", cfg.user_login);
        AppError::auth("Brak tokenu logowania")
    })?;

    record_sync_attempt(state, source);
    state
        .sync_engine
        .sync_all(app, &cfg, &token.token, source)
        .await
}

/// Sync scheduler loop — reacts instantly to file changes (Synology Drive-style)
/// instead of polling on a fixed interval.
///
/// Uses `tokio::select!` to await whichever fires first:
///   - watcher channel (file changed locally → sync immediately after debounce)
///   - interval timer (periodic full sync as safety net)
///   - startup trigger (first sync after login)
async fn run_scheduler_loop(app: tauri::AppHandle) {
    // Take the watcher receiver so we can await on it directly.
    let mut watcher_rx: Option<tokio::sync::mpsc::Receiver<()>> = {
        let state = app.state::<AppState>();
        let mut watcher = state.watcher();
        watcher.take_receiver()
    };

    // Debounce: after detecting a file change, wait this long for more
    // changes to settle before triggering sync (e.g. user copying 10 files).
    const WATCHER_DEBOUNCE: Duration = Duration::from_secs(2);

    let mut interval = tokio::time::interval(Duration::from_secs(5));

    loop {
        // Wait for either: interval tick, file change, or both.
        let source = if let Some(rx) = &mut watcher_rx {
            tokio::select! {
                _ = interval.tick() => "interval_or_startup",
                _ = rx.recv() => {
                    // Debounce: wait a bit for more changes to settle,
                    // draining any events that arrive during the window.
                    tokio::time::sleep(WATCHER_DEBOUNCE).await;
                    while rx.try_recv().is_ok() {}
                    "watcher"
                }
            }
        } else {
            interval.tick().await;
            "interval_or_startup"
        };

        let state = app.state::<AppState>();
        let cfg = state.config().clone();
        if !cfg.is_configured() {
            continue;
        }

        // Determine what kind of sync to run.
        let now = Instant::now();
        let final_source = if source == "watcher" {
            // Watcher-triggered: skip if too soon after last watcher sync
            let too_soon = {
                let scheduler = state.scheduler();
                scheduler
                    .last_watch_sync_attempt
                    .map(|last| now.duration_since(last) < Duration::from_secs(3))
                    .unwrap_or(false)
            };
            if too_soon {
                continue;
            }
            "watcher"
        } else {
            // Interval tick — check if startup or interval sync is due
            let (should_startup, should_interval) = {
                let scheduler = state.scheduler();
                let startup = cfg.sync_on_startup && !scheduler.startup_sync_done;
                let interval_due = scheduler
                    .last_sync_attempt
                    .map(|last| {
                        now.duration_since(last) >= Duration::from_secs(cfg.sync_interval_secs)
                    })
                    .unwrap_or(false);
                (startup, interval_due)
            };

            if should_startup {
                "startup"
            } else if should_interval {
                "interval"
            } else {
                continue;
            }
        };

        let result = run_sync_once(&app, &state, final_source).await;

        if final_source == "startup" {
            let mut scheduler = state.scheduler();
            scheduler.startup_sync_done = true;
        }

        if let Err(err) = result {
            log::warn!("Background sync failed: {}", err);
        }
    }
}

// ==================== Tauri Commands ====================

/// Login with login and password
#[tauri::command]
async fn login(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    server_url: String,
    login: String,
    password: String,
) -> Result<String, String> {
    validate_server_url(&server_url).map_err(|e| e.to_string())?;

    let response = auth::login(&server_url, &login, &password)
        .await
        .map_err(|e| e.to_string())?;
    persist_login(
        &app,
        &state,
        server_url,
        login,
        response.token,
        response.user,
    )
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
        response.user.login.clone(),
        response.token,
        response.user,
    )
    .map_err(|e| e.to_string())
}

fn persist_login(
    app: &tauri::AppHandle,
    state: &State<'_, AppState>,
    server_url: String,
    login: String,
    api_token: String,
    user: auth::LoginUser,
) -> AppResult<String> {
    log::info!(
        "persist_login: storing credentials for {} at {}",
        login,
        server_url
    );
    // Store token in keychain
    let token = auth::AuthToken {
        token: api_token,
        token_type: auth::TokenType::Sanctum,
        expires_at: None,
    };
    auth::store_token(&login, &token)?;

    // Verify token was stored — fail login if keychain didn't persist
    match auth::get_token(&login) {
        Ok(Some(_)) => log::info!("persist_login: token verification OK"),
        Ok(None) => {
            log::error!("persist_login: token verification FAILED - not found after store!");
            return Err(AppError::auth("Nie udało się zapisać tokenu w systemie. Sprawdź uprawnienia do Windows Credential Manager."));
        }
        Err(e) => {
            log::error!("persist_login: token verification FAILED - {}", e);
            return Err(AppError::auth(format!(
                "Weryfikacja tokenu nie powiodła się: {}",
                e
            )));
        }
    }

    let user_json = serde_json::to_string(&user)
        .map_err(|e| AppError::internal(format!("Błąd serializacji użytkownika: {}", e)))?;

    // Update config and persist
    {
        let mut cfg = state.config();
        cfg.server_url = server_url;
        cfg.user_login = login;
        cfg.tenant_id = user.tenant_id.unwrap_or_default();
        config::save_config(app, &cfg)?;
    }

    reset_scheduler_for_login_or_config(state);
    configure_watcher_for_current_config(state)?;
    state.sync_engine.set_idle();

    Ok(user_json)
}

/// Logout and remove stored credentials
#[tauri::command]
fn logout(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let login = state.config().user_login.clone();
    if !login.is_empty() {
        auth::remove_token(&login).map_err(|e| e.to_string())?;
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
    run_sync_once(&app, &state, "manual")
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

/// Get debug mode status and log file path
#[tauri::command]
fn get_debug_info(state: State<'_, AppState>) -> (bool, String) {
    (
        state.debug_enabled.load(Ordering::Relaxed),
        state.log_path.to_string_lossy().to_string(),
    )
}

/// Open the current log file in the system default app
#[tauri::command]
fn open_log_file(state: State<'_, AppState>) -> Result<(), String> {
    let log_path = normalize_existing_path(&state.log_path).map_err(|e| e.to_string())?;
    if !log_path.is_file() {
        return Err("Plik logu nie istnieje".to_string());
    }

    open::that(&log_path).map_err(|e| format!("Nie udało się otworzyć pliku logu: {}", e))
}

/// Toggle debug mode
#[tauri::command]
fn set_debug_mode(state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    state.debug_enabled.store(enabled, Ordering::Relaxed);
    if enabled {
        log::set_max_level(log::LevelFilter::Debug);
        log::info!("Debug mode enabled, log file: {}", state.log_path.display());
    } else {
        log::set_max_level(log::LevelFilter::Info);
        log::info!("Debug mode disabled");
    }
    Ok(())
}

/// Read log file contents (last N lines, reads only tail of file)
#[tauri::command]
fn get_log_contents(
    state: State<'_, AppState>,
    max_lines: Option<usize>,
) -> Result<String, String> {
    use std::io::{Read, Seek, SeekFrom};
    let limit = max_lines.unwrap_or(200);
    let mut file = std::fs::File::open(&state.log_path)
        .map_err(|e| format!("Nie udało się odczytać logów: {}", e))?;
    let file_len = file.metadata().map(|m| m.len()).unwrap_or(0);
    // Read at most 256KB from the end
    let read_from = file_len.saturating_sub(256 * 1024);
    file.seek(SeekFrom::Start(read_from))
        .map_err(|e| format!("Seek error: {}", e))?;
    let mut buf = String::new();
    file.read_to_string(&mut buf)
        .map_err(|e| format!("Read error: {}", e))?;
    let lines: Vec<&str> = buf.lines().collect();
    let start = if lines.len() > limit {
        lines.len() - limit
    } else {
        0
    };
    Ok(lines[start..].join("\n"))
}

/// Pick a folder using native OS dialog
#[tauri::command]
async fn pick_folder(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let handle = app.clone();
    tokio::task::spawn_blocking(move || {
        let folder = handle
            .dialog()
            .file()
            .set_title("Wybierz folder synchronizacji")
            .blocking_pick_folder();
        Ok(folder.map(|f| f.to_string()))
    })
    .await
    .map_err(|e| format!("Dialog task failed: {}", e))?
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

fn hide_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
}

fn configure_tray(app: &tauri::App) {
    let initial_status = app.state::<AppState>().sync_engine.get_status();
    update_tray_icon(app.handle(), &initial_status);

    let Some(tray) = app.tray_by_id("main-tray") else {
        log::warn!("Tray icon unavailable; leaving main window visible");
        show_window(app.handle());
        return;
    };

    let Ok(menu) = build_tray_menu(app.handle(), &initial_status) else {
        log::warn!("Failed to build tray menu; leaving main window visible");
        show_window(app.handle());
        return;
    };

    if let Err(err) = tray.set_menu(Some(menu)) {
        log::warn!("Failed to attach tray menu: {}", err);
        show_window(app.handle());
        return;
    }

    if let Err(err) = tray.set_show_menu_on_left_click(false) {
        log::warn!("Failed to configure tray click behavior: {}", err);
    }

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

    tray.on_menu_event(|app, event| {
        handle_tray_menu_event(app, event);
    });

    hide_window(app.handle());
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
    let open_personal = MenuItem::with_id(
        app,
        "open_personal",
        "Otwórz Moje pliki",
        true,
        None::<&str>,
    )?;
    let open_shared = MenuItem::with_id(
        app,
        "open_shared",
        "Otwórz Udostępnione",
        true,
        None::<&str>,
    )?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let sync_now = MenuItem::with_id(
        app,
        "sync_now",
        "Synchronizuj teraz",
        !matches!(status, SyncStatus::Syncing),
        None::<&str>,
    )?;
    let sep3 = PredefinedMenuItem::separator(app)?;
    let settings_item = MenuItem::with_id(app, "settings", "Ustawienia...", true, None::<&str>)?;
    let sep4 = PredefinedMenuItem::separator(app)?;
    let quit_item = MenuItem::with_id(app, "quit", "Zakończ", true, None::<&str>)?;

    Menu::with_items(
        app,
        &[
            &status_item,
            &sep1,
            &open_personal,
            &open_shared,
            &sep2,
            &sync_now,
            &sep3,
            &settings_item,
            &sep4,
            &quit_item,
        ],
    )
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
                let login = cfg.user_login.clone();
                if let Ok(Some(token)) = auth::get_token(&login) {
                    let engine = state.sync_engine.clone();
                    // Update tray icon to syncing
                    update_tray_icon(&app, &SyncStatus::Syncing);
                    let result_status = match engine.sync_all(&app, &cfg, &token.token, "manual").await {
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
    // Safety net for WebKitGTK on Linux — disable DMA-BUF renderer to avoid
    // edge-case GPU issues. The main EGL fix is in the AppImage packaging
    // (bundled wayland/EGL libs are removed so the host's matching set is used).
    // SAFETY: env vars set before any threads are spawned (single-threaded at this point)
    #[cfg(target_os = "linux")]
    {
        if std::env::var("WEBKIT_DISABLE_DMABUF_RENDERER").is_err() {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
    }

    // Setup log file in app data directory
    let log_dir = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.veloryn.cloudfile")
        .join("logs");
    let _ = std::fs::create_dir_all(&log_dir);
    let log_path = log_dir.join("veloryn-cloudfile.log");

    use simplelog::{CombinedLogger, Config as LogConfig, LevelFilter, WriteLogger};
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .expect("Failed to open log file");
    CombinedLogger::init(vec![WriteLogger::new(
        LevelFilter::Info,
        LogConfig::default(),
        log_file,
    )])
    .expect("Failed to initialize logger");

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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(AppState {
            config: Mutex::new(AppConfig::default()),
            sync_engine: sync_engine.clone(),
            watcher: Mutex::new(watcher::FileWatcher::new()),
            scheduler: Mutex::new(SchedulerState::default()),
            debug_enabled: AtomicBool::new(false),
            log_path: log_path.clone(),
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
            get_debug_info,
            open_log_file,
            set_debug_mode,
            get_log_contents,
        ])
        .setup(|app| {
            // Load persisted config from store
            if let Some(saved_config) = config::load_config(app.handle()) {
                let state = app.state::<AppState>();
                let is_configured = saved_config.is_configured();
                *state.config() = saved_config;
                if is_configured {
                    state.sync_engine.set_idle();
                }
                if let Err(err) = configure_watcher_for_current_config(&state) {
                    log::warn!("Failed to configure watcher on startup: {}", err);
                }
            }

            {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(run_scheduler_loop(handle));
            }

            // Background update check — once per day
            {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    use tauri_plugin_updater::UpdaterExt;
                    // Initial delay: 30 seconds after startup
                    tokio::time::sleep(Duration::from_secs(30)).await;
                    let mut interval = tokio::time::interval(Duration::from_secs(86400));
                    loop {
                        interval.tick().await;
                        match handle.updater() {
                            Ok(updater) => match updater.check().await {
                                Ok(Some(update)) => {
                                    log::info!("Update available: v{}", update.version);
                                    let _ = handle.emit("update-available", &update.version);
                                }
                                Ok(None) => log::debug!("No update available"),
                                Err(e) => log::warn!("Update check failed: {}", e),
                            },
                            Err(e) => log::warn!("Updater init failed: {}", e),
                        }
                    }
                });
            }

            // On Linux: disable hardware acceleration to avoid EGL/GPU issues.
            // The WEBKIT_DISABLE_DMABUF_RENDERER env var is set at the top of main().
            #[cfg(target_os = "linux")]
            {
                if let Some(window) = app.get_webview_window("main") {
                    window
                        .with_webview(|webview| {
                            use webkit2gtk::SettingsExt;
                            use webkit2gtk::WebViewExt;
                            if let Some(settings) = WebViewExt::settings(&webview.inner()) {
                                SettingsExt::set_hardware_acceleration_policy(
                                    &settings,
                                    webkit2gtk::HardwareAccelerationPolicy::Never,
                                );
                            }
                        })
                        .ok();
                }
            }

            // Setup tray icon if available; otherwise fallback to a visible window.
            configure_tray(app);

            log::info!("Veloryn CloudFile started");
            Ok(())
        })
        .on_window_event(|window, event| {
            // Hide window instead of closing (keep in tray)
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.app_handle().tray_by_id("main-tray").is_some() {
                    let _ = window.hide();
                    api.prevent_close();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Veloryn CloudFile");
}
