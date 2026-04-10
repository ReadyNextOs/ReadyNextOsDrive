use crate::error::{AppError, AppResult};
use rusqlite::{params, Connection, OptionalExtension};
use std::sync::{Arc, Mutex};

pub type DbPool = Arc<Mutex<Connection>>;

// ==================== Structs ====================

#[derive(Debug, Clone, serde::Serialize)]
pub struct FileState {
    pub path: String,
    pub sync_zone: String,
    pub local_hash: Option<String>,
    pub local_mtime: Option<i64>,
    pub local_size: Option<i64>,
    pub local_exists: bool,
    pub remote_etag: Option<String>,
    pub remote_mtime: Option<i64>,
    pub remote_size: Option<i64>,
    pub remote_exists: bool,
    pub sync_status: String,
    pub last_synced_hash: Option<String>,
    pub last_synced_mtime: Option<i64>,
    pub last_synced_etag: Option<String>,
    pub last_synced_at: Option<String>,
    pub error_message: Option<String>,
    pub retry_count: i32,
}

#[derive(Debug, Clone)]
pub struct SyncRunStats {
    pub files_uploaded: i32,
    pub files_downloaded: i32,
    pub files_deleted: i32,
    pub files_conflicted: i32,
    pub bytes_transferred: i64,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct LocalTrashEntry {
    pub id: Option<i64>,
    pub original_path: String,
    pub trash_path: String,
    pub sync_zone: String,
    pub size_bytes: Option<i64>,
    pub deleted_at: Option<String>,
    pub auto_delete_at: String,
}

// ==================== Init ====================

/// Open or create the SQLite database with WAL mode.
/// Location: ~/.local/share/com.veloryn.cloudfile/sync.db
/// On macOS: ~/Library/Application Support/com.veloryn.cloudfile/sync.db
pub fn init_db() -> AppResult<DbPool> {
    let data_dir = dirs::data_local_dir()
        .ok_or_else(|| AppError::io("Cannot determine local data directory"))?
        .join("com.veloryn.cloudfile");

    std::fs::create_dir_all(&data_dir)
        .map_err(|e| AppError::io(format!("Failed to create data directory: {}", e)))?;

    let db_path = data_dir.join("sync.db");

    let conn = Connection::open(&db_path)
        .map_err(|e| AppError::io(format!("Failed to open database: {}", e)))?;

    // Enable WAL mode for better concurrent read performance
    conn.pragma_update(None, "journal_mode", "WAL")
        .map_err(|e| AppError::io(format!("Failed to set WAL mode: {}", e)))?;

    // Enable foreign keys
    conn.pragma_update(None, "foreign_keys", "ON")
        .map_err(|e| AppError::io(format!("Failed to enable foreign keys: {}", e)))?;

    migrate(&conn)?;

    Ok(Arc::new(Mutex::new(conn)))
}

// ==================== Migrations ====================

/// Run schema migrations (create tables if not exist).
pub fn migrate(conn: &Connection) -> AppResult<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS file_state (
            path TEXT NOT NULL,
            sync_zone TEXT NOT NULL CHECK (sync_zone IN ('personal', 'shared')),
            local_hash TEXT,
            local_mtime INTEGER,
            local_size INTEGER,
            local_exists INTEGER NOT NULL DEFAULT 1,
            remote_etag TEXT,
            remote_mtime INTEGER,
            remote_size INTEGER,
            remote_exists INTEGER NOT NULL DEFAULT 1,
            sync_status TEXT NOT NULL DEFAULT 'unknown'
                CHECK (sync_status IN (
                    'synced', 'local_new', 'local_modified', 'local_deleted',
                    'remote_new', 'remote_modified', 'remote_deleted',
                    'conflict', 'error', 'unknown'
                )),
            last_synced_hash TEXT,
            last_synced_mtime INTEGER,
            last_synced_etag TEXT,
            last_synced_at TEXT,
            error_message TEXT,
            retry_count INTEGER NOT NULL DEFAULT 0,
            last_error_at TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now')),
            PRIMARY KEY (path, sync_zone)
        );

        CREATE INDEX IF NOT EXISTS idx_file_state_dirty
            ON file_state(sync_status) WHERE sync_status NOT IN ('synced', 'unknown');

        CREATE TABLE IF NOT EXISTS local_trash (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            original_path TEXT NOT NULL,
            trash_path TEXT NOT NULL,
            sync_zone TEXT NOT NULL,
            size_bytes INTEGER,
            deleted_at TEXT NOT NULL DEFAULT (datetime('now')),
            auto_delete_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS sync_run (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            started_at TEXT NOT NULL,
            completed_at TEXT,
            status TEXT NOT NULL CHECK (status IN ('running', 'success', 'error', 'partial')),
            source TEXT,
            files_uploaded INTEGER DEFAULT 0,
            files_downloaded INTEGER DEFAULT 0,
            files_deleted INTEGER DEFAULT 0,
            files_conflicted INTEGER DEFAULT 0,
            bytes_transferred INTEGER DEFAULT 0,
            error_message TEXT,
            duration_ms INTEGER
        );

        INSERT OR IGNORE INTO schema_version (version) VALUES (1);
        ",
    )
    .map_err(|e| AppError::io(format!("Migration failed: {}", e)))?;

    Ok(())
}

// ==================== File State CRUD ====================

pub fn get_file_state(pool: &DbPool, path: &str, zone: &str) -> AppResult<Option<FileState>> {
    let conn = pool
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let mut stmt = conn
        .prepare(
            "SELECT path, sync_zone, local_hash, local_mtime, local_size, local_exists,
                    remote_etag, remote_mtime, remote_size, remote_exists,
                    sync_status, last_synced_hash, last_synced_mtime, last_synced_etag, last_synced_at,
                    error_message, retry_count
             FROM file_state
             WHERE path = ?1 AND sync_zone = ?2",
        )
        .map_err(|e| AppError::io(format!("Failed to prepare statement: {}", e)))?;

    let result = stmt
        .query_row(params![path, zone], |row| {
            Ok(FileState {
                path: row.get(0)?,
                sync_zone: row.get(1)?,
                local_hash: row.get(2)?,
                local_mtime: row.get(3)?,
                local_size: row.get(4)?,
                local_exists: row.get::<_, i64>(5)? != 0,
                remote_etag: row.get(6)?,
                remote_mtime: row.get(7)?,
                remote_size: row.get(8)?,
                remote_exists: row.get::<_, i64>(9)? != 0,
                sync_status: row.get(10)?,
                last_synced_hash: row.get(11)?,
                last_synced_mtime: row.get(12)?,
                last_synced_etag: row.get(13)?,
                last_synced_at: row.get(14)?,
                error_message: row.get(15)?,
                retry_count: row.get(16)?,
            })
        })
        .optional()
        .map_err(|e| AppError::io(format!("Failed to query file state: {}", e)))?;

    Ok(result)
}

pub fn upsert_file_state(pool: &DbPool, state: &FileState) -> AppResult<()> {
    let conn = pool
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    conn.execute(
        "INSERT INTO file_state (
            path, sync_zone, local_hash, local_mtime, local_size, local_exists,
            remote_etag, remote_mtime, remote_size, remote_exists,
            sync_status, last_synced_hash, last_synced_mtime, last_synced_etag, last_synced_at,
            error_message, retry_count, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, datetime('now'))
        ON CONFLICT(path, sync_zone) DO UPDATE SET
            local_hash = excluded.local_hash,
            local_mtime = excluded.local_mtime,
            local_size = excluded.local_size,
            local_exists = excluded.local_exists,
            remote_etag = excluded.remote_etag,
            remote_mtime = excluded.remote_mtime,
            remote_size = excluded.remote_size,
            remote_exists = excluded.remote_exists,
            sync_status = excluded.sync_status,
            last_synced_hash = excluded.last_synced_hash,
            last_synced_mtime = excluded.last_synced_mtime,
            last_synced_etag = excluded.last_synced_etag,
            last_synced_at = excluded.last_synced_at,
            error_message = excluded.error_message,
            retry_count = excluded.retry_count,
            updated_at = datetime('now')",
        params![
            state.path,
            state.sync_zone,
            state.local_hash,
            state.local_mtime,
            state.local_size,
            state.local_exists as i64,
            state.remote_etag,
            state.remote_mtime,
            state.remote_size,
            state.remote_exists as i64,
            state.sync_status,
            state.last_synced_hash,
            state.last_synced_mtime,
            state.last_synced_etag,
            state.last_synced_at,
            state.error_message,
            state.retry_count,
        ],
    )
    .map_err(|e| AppError::io(format!("Failed to upsert file state: {}", e)))?;

    Ok(())
}

pub fn delete_file_state(pool: &DbPool, path: &str, zone: &str) -> AppResult<()> {
    let conn = pool
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    conn.execute(
        "DELETE FROM file_state WHERE path = ?1 AND sync_zone = ?2",
        params![path, zone],
    )
    .map_err(|e| AppError::io(format!("Failed to delete file state: {}", e)))?;

    Ok(())
}

pub fn list_files_by_zone(pool: &DbPool, zone: &str) -> AppResult<Vec<FileState>> {
    let conn = pool
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let mut stmt = conn
        .prepare(
            "SELECT path, sync_zone, local_hash, local_mtime, local_size, local_exists,
                    remote_etag, remote_mtime, remote_size, remote_exists,
                    sync_status, last_synced_hash, last_synced_mtime, last_synced_etag, last_synced_at,
                    error_message, retry_count
             FROM file_state
             WHERE sync_zone = ?1
             ORDER BY path",
        )
        .map_err(|e| AppError::io(format!("Failed to prepare statement: {}", e)))?;

    let rows = stmt
        .query_map(params![zone], |row| {
            Ok(FileState {
                path: row.get(0)?,
                sync_zone: row.get(1)?,
                local_hash: row.get(2)?,
                local_mtime: row.get(3)?,
                local_size: row.get(4)?,
                local_exists: row.get::<_, i64>(5)? != 0,
                remote_etag: row.get(6)?,
                remote_mtime: row.get(7)?,
                remote_size: row.get(8)?,
                remote_exists: row.get::<_, i64>(9)? != 0,
                sync_status: row.get(10)?,
                last_synced_hash: row.get(11)?,
                last_synced_mtime: row.get(12)?,
                last_synced_etag: row.get(13)?,
                last_synced_at: row.get(14)?,
                error_message: row.get(15)?,
                retry_count: row.get(16)?,
            })
        })
        .map_err(|e| AppError::io(format!("Failed to list files by zone: {}", e)))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::io(format!("Failed to collect file states: {}", e)))
}

pub fn get_dirty_files(pool: &DbPool) -> AppResult<Vec<FileState>> {
    let conn = pool
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let mut stmt = conn
        .prepare(
            "SELECT path, sync_zone, local_hash, local_mtime, local_size, local_exists,
                    remote_etag, remote_mtime, remote_size, remote_exists,
                    sync_status, last_synced_hash, last_synced_mtime, last_synced_etag, last_synced_at,
                    error_message, retry_count
             FROM file_state
             WHERE sync_status NOT IN ('synced', 'unknown')
             ORDER BY path",
        )
        .map_err(|e| AppError::io(format!("Failed to prepare statement: {}", e)))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(FileState {
                path: row.get(0)?,
                sync_zone: row.get(1)?,
                local_hash: row.get(2)?,
                local_mtime: row.get(3)?,
                local_size: row.get(4)?,
                local_exists: row.get::<_, i64>(5)? != 0,
                remote_etag: row.get(6)?,
                remote_mtime: row.get(7)?,
                remote_size: row.get(8)?,
                remote_exists: row.get::<_, i64>(9)? != 0,
                sync_status: row.get(10)?,
                last_synced_hash: row.get(11)?,
                last_synced_mtime: row.get(12)?,
                last_synced_etag: row.get(13)?,
                last_synced_at: row.get(14)?,
                error_message: row.get(15)?,
                retry_count: row.get(16)?,
            })
        })
        .map_err(|e| AppError::io(format!("Failed to query dirty files: {}", e)))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::io(format!("Failed to collect dirty files: {}", e)))
}

pub fn mark_synced(pool: &DbPool, path: &str, zone: &str, remote_etag: &str) -> AppResult<()> {
    let conn = pool
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    conn.execute(
        "UPDATE file_state
         SET sync_status = 'synced',
             last_synced_etag = ?3,
             last_synced_hash = local_hash,
             last_synced_mtime = local_mtime,
             last_synced_at = datetime('now'),
             error_message = NULL,
             retry_count = 0,
             last_error_at = NULL,
             updated_at = datetime('now')
         WHERE path = ?1 AND sync_zone = ?2",
        params![path, zone, remote_etag],
    )
    .map_err(|e| AppError::io(format!("Failed to mark file as synced: {}", e)))?;

    Ok(())
}

// ==================== Sync Run Tracking ====================

pub fn start_sync_run(pool: &DbPool, source: &str) -> AppResult<i64> {
    let conn = pool
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    conn.execute(
        "INSERT INTO sync_run (started_at, status, source)
         VALUES (datetime('now'), 'running', ?1)",
        params![source],
    )
    .map_err(|e| AppError::io(format!("Failed to start sync run: {}", e)))?;

    Ok(conn.last_insert_rowid())
}

pub fn complete_sync_run(
    pool: &DbPool,
    id: i64,
    status: &str,
    stats: &SyncRunStats,
) -> AppResult<()> {
    let conn = pool
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    conn.execute(
        "UPDATE sync_run
         SET completed_at = datetime('now'),
             status = ?2,
             files_uploaded = ?3,
             files_downloaded = ?4,
             files_deleted = ?5,
             files_conflicted = ?6,
             bytes_transferred = ?7,
             error_message = ?8,
             duration_ms = CAST(
                 (julianday(datetime('now')) - julianday(started_at)) * 86400000 AS INTEGER
             )
         WHERE id = ?1",
        params![
            id,
            status,
            stats.files_uploaded,
            stats.files_downloaded,
            stats.files_deleted,
            stats.files_conflicted,
            stats.bytes_transferred,
            stats.error_message,
        ],
    )
    .map_err(|e| AppError::io(format!("Failed to complete sync run: {}", e)))?;

    Ok(())
}

/// A persisted sync run record.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SyncRunEntry {
    pub id: i64,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub status: String,
    pub source: Option<String>,
    pub files_uploaded: i32,
    pub files_downloaded: i32,
    pub files_deleted: i32,
    pub files_conflicted: i32,
    pub bytes_transferred: i64,
    pub error_message: Option<String>,
    pub duration_ms: Option<i64>,
}

/// List recent sync runs from SQLite (most recent first).
pub fn list_sync_runs(pool: &DbPool, limit: usize) -> AppResult<Vec<SyncRunEntry>> {
    let conn = pool
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let mut stmt = conn
        .prepare(
            "SELECT id, started_at, completed_at, status, source,
                    files_uploaded, files_downloaded, files_deleted,
                    files_conflicted, bytes_transferred, error_message, duration_ms
             FROM sync_run
             ORDER BY started_at DESC
             LIMIT ?1",
        )
        .map_err(|e| AppError::io(format!("Failed to prepare sync_run query: {}", e)))?;

    let entries = stmt
        .query_map(params![limit as i64], |row| {
            Ok(SyncRunEntry {
                id: row.get(0)?,
                started_at: row.get(1)?,
                completed_at: row.get(2)?,
                status: row.get(3)?,
                source: row.get(4)?,
                files_uploaded: row.get(5)?,
                files_downloaded: row.get(6)?,
                files_deleted: row.get(7)?,
                files_conflicted: row.get(8)?,
                bytes_transferred: row.get(9)?,
                error_message: row.get(10)?,
                duration_ms: row.get(11)?,
            })
        })
        .map_err(|e| AppError::io(format!("Failed to query sync_run: {}", e)))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::io(format!("Failed to read sync_run rows: {}", e)))?;

    Ok(entries)
}

// ==================== Local Trash ====================

pub fn add_to_local_trash(pool: &DbPool, entry: &LocalTrashEntry) -> AppResult<()> {
    let conn = pool
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    conn.execute(
        "INSERT INTO local_trash (original_path, trash_path, sync_zone, size_bytes, auto_delete_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            entry.original_path,
            entry.trash_path,
            entry.sync_zone,
            entry.size_bytes,
            entry.auto_delete_at,
        ],
    )
    .map_err(|e| AppError::io(format!("Failed to add to local trash: {}", e)))?;

    Ok(())
}

pub fn list_local_trash(pool: &DbPool) -> AppResult<Vec<LocalTrashEntry>> {
    let conn = pool
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let mut stmt = conn
        .prepare(
            "SELECT id, original_path, trash_path, sync_zone, size_bytes, deleted_at, auto_delete_at
             FROM local_trash
             ORDER BY deleted_at DESC",
        )
        .map_err(|e| AppError::io(format!("Failed to prepare statement: {}", e)))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(LocalTrashEntry {
                id: row.get(0)?,
                original_path: row.get(1)?,
                trash_path: row.get(2)?,
                sync_zone: row.get(3)?,
                size_bytes: row.get(4)?,
                deleted_at: row.get(5)?,
                auto_delete_at: row.get(6)?,
            })
        })
        .map_err(|e| AppError::io(format!("Failed to list local trash: {}", e)))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::io(format!("Failed to collect trash entries: {}", e)))
}

pub fn remove_from_local_trash(pool: &DbPool, id: i64) -> AppResult<()> {
    let conn = pool
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    conn.execute("DELETE FROM local_trash WHERE id = ?1", params![id])
        .map_err(|e| AppError::io(format!("Failed to remove trash entry: {}", e)))?;

    Ok(())
}

pub fn cleanup_expired_trash(pool: &DbPool) -> AppResult<usize> {
    let conn = pool
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let count = conn
        .execute(
            "DELETE FROM local_trash WHERE auto_delete_at <= datetime('now')",
            [],
        )
        .map_err(|e| AppError::io(format!("Failed to cleanup expired trash: {}", e)))?;

    Ok(count)
}
