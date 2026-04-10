use crate::db::FileState;
use std::collections::HashMap;

/// Action to perform during sync, determined by 3-way diff.
#[derive(Debug, Clone, serde::Serialize)]
pub enum SyncAction {
    /// Upload local file to remote.
    Upload {
        local_path: String,
        remote_path: String,
    },
    /// Download remote file to local.
    Download {
        remote_path: String,
        local_path: String,
    },
    /// Delete the local copy (remote was deleted).
    DeleteLocal {
        local_path: String,
    },
    /// Delete the remote copy (local was deleted).
    DeleteRemote {
        remote_path: String,
    },
    /// Conflict — both sides modified since last sync.
    Conflict {
        local_path: String,
        remote_path: String,
        conflict_type: ConflictType,
    },
    /// No action needed — file is in sync.
    Skip,
}

#[derive(Debug, Clone, serde::Serialize)]
pub enum ConflictType {
    /// Both local and remote were modified independently.
    BothModified,
    /// Deleted locally but modified on remote.
    DeletedLocallyModifiedRemotely,
    /// Deleted on remote but modified locally.
    DeletedRemotelyModifiedLocally,
}

/// Snapshot of a local file from filesystem scan.
#[derive(Debug, Clone)]
pub struct LocalFileInfo {
    pub path: String,
    pub hash: String,
    pub mtime: i64,
    pub size: i64,
}

/// Snapshot of a remote file from WebDAV PROPFIND.
#[derive(Debug, Clone)]
pub struct RemoteFileInfo {
    pub path: String,
    pub etag: Option<String>,
    pub mtime: Option<i64>,
    pub size: Option<i64>,
}

/// Result of the 3-way diff for a single file.
#[derive(Debug, Clone)]
pub struct DiffResult {
    pub path: String,
    pub action: SyncAction,
}

/// Compute sync actions by comparing local files, remote files, and last-known state.
///
/// 3-way diff algorithm:
/// | Local      | Remote     | Last Known | Action                    |
/// |------------|------------|------------|---------------------------|
/// | new        | missing    | missing    | Upload                    |
/// | missing    | new        | missing    | Download                  |
/// | modified   | unmodified | old        | Upload                    |
/// | unmodified | modified   | old        | Download                  |
/// | modified   | modified   | old        | CONFLICT                  |
/// | missing    | exists     | existed    | DeleteLocal was done / DL |
/// | exists     | missing    | existed    | DeleteRemote was done / U |
/// | missing    | missing    | existed    | Already synced delete     |
pub fn compute_diff(
    local_files: &[LocalFileInfo],
    remote_files: &[RemoteFileInfo],
    known_states: &[FileState],
    sync_zone: &str,
    local_base: &str,
    remote_base: &str,
) -> Vec<DiffResult> {
    // Index by relative path for O(1) lookup
    let local_map: HashMap<&str, &LocalFileInfo> =
        local_files.iter().map(|f| (f.path.as_str(), f)).collect();
    let remote_map: HashMap<&str, &RemoteFileInfo> =
        remote_files.iter().map(|f| (f.path.as_str(), f)).collect();
    let known_map: HashMap<&str, &FileState> = known_states
        .iter()
        .filter(|s| s.sync_zone == sync_zone)
        .map(|s| (s.path.as_str(), s))
        .collect();

    // Collect all unique paths across all three sources
    let mut all_paths: Vec<&str> = Vec::new();
    for key in local_map.keys() {
        all_paths.push(key);
    }
    for key in remote_map.keys() {
        if !local_map.contains_key(key) {
            all_paths.push(key);
        }
    }
    for key in known_map.keys() {
        if !local_map.contains_key(key) && !remote_map.contains_key(key) {
            all_paths.push(key);
        }
    }
    all_paths.sort();

    let mut results = Vec::with_capacity(all_paths.len());

    for path in all_paths {
        let local = local_map.get(path).copied();
        let remote = remote_map.get(path).copied();
        let known = known_map.get(path).copied();

        let action = diff_file(local, remote, known, path, local_base, remote_base);
        results.push(DiffResult {
            path: path.to_string(),
            action,
        });
    }

    // Filter out Skip actions
    results.retain(|r| !matches!(r.action, SyncAction::Skip));
    results
}

/// Determine the sync action for a single file based on 3-way comparison.
fn diff_file(
    local: Option<&LocalFileInfo>,
    remote: Option<&RemoteFileInfo>,
    known: Option<&FileState>,
    path: &str,
    local_base: &str,
    remote_base: &str,
) -> SyncAction {
    let local_path = format!("{}/{}", local_base, path);
    let remote_path = format!("{}/{}", remote_base, path);

    match (local, remote, known) {
        // New local file, not on remote, never synced → Upload
        (Some(_), None, None) => SyncAction::Upload {
            local_path,
            remote_path,
        },

        // New remote file, not local, never synced → Download
        (None, Some(_), None) => SyncAction::Download {
            remote_path,
            local_path,
        },

        // Both new (never synced, exists on both) → Conflict
        (Some(_), Some(_), None) => SyncAction::Conflict {
            local_path,
            remote_path,
            conflict_type: ConflictType::BothModified,
        },

        // Both exist, previously synced → check modifications
        (Some(l), Some(r), Some(k)) => {
            let local_changed = is_local_changed(l, k);
            let remote_changed = is_remote_changed(r, k);

            match (local_changed, remote_changed) {
                (false, false) => SyncAction::Skip,
                (true, false) => SyncAction::Upload {
                    local_path,
                    remote_path,
                },
                (false, true) => SyncAction::Download {
                    remote_path,
                    local_path,
                },
                (true, true) => SyncAction::Conflict {
                    local_path,
                    remote_path,
                    conflict_type: ConflictType::BothModified,
                },
            }
        }

        // Local exists, remote missing, previously synced → remote was deleted
        (Some(l), None, Some(k)) => {
            if is_local_changed(l, k) {
                // Local was modified after last sync, but remote deleted
                SyncAction::Conflict {
                    local_path,
                    remote_path,
                    conflict_type: ConflictType::DeletedRemotelyModifiedLocally,
                }
            } else {
                // Local unchanged → honor remote delete
                SyncAction::DeleteLocal { local_path }
            }
        }

        // Remote exists, local missing, previously synced → local was deleted
        (None, Some(r), Some(k)) => {
            if is_remote_changed(r, k) {
                // Remote was modified after last sync, but local deleted
                SyncAction::Conflict {
                    local_path,
                    remote_path,
                    conflict_type: ConflictType::DeletedLocallyModifiedRemotely,
                }
            } else {
                // Remote unchanged → honor local delete
                SyncAction::DeleteRemote { remote_path }
            }
        }

        // Both missing — either already synced delete or never existed
        (None, None, _) => SyncAction::Skip,
    }
}

/// Check if local file changed compared to last synced state.
fn is_local_changed(local: &LocalFileInfo, known: &FileState) -> bool {
    // Compare by hash if available (most reliable)
    if let Some(ref known_hash) = known.last_synced_hash {
        return local.hash != *known_hash;
    }

    // Fallback: compare by size + mtime
    if let Some(known_size) = known.local_size {
        if local.size != known_size {
            return true;
        }
    }

    if let Some(known_mtime) = known.last_synced_mtime {
        if local.mtime != known_mtime {
            return true;
        }
    }

    false
}

/// Check if remote file changed compared to last synced state.
fn is_remote_changed(remote: &RemoteFileInfo, known: &FileState) -> bool {
    // Compare by etag (most reliable for remote)
    if let (Some(ref remote_etag), Some(ref known_etag)) = (&remote.etag, &known.last_synced_etag)
    {
        return remote_etag != known_etag;
    }

    // Fallback: compare by size
    if let (Some(remote_size), Some(known_size)) = (remote.size, known.remote_size) {
        if remote_size != known_size {
            return true;
        }
    }

    // Fallback: compare by mtime
    if let (Some(remote_mtime), Some(known_mtime)) = (remote.mtime, known.remote_mtime) {
        if remote_mtime != known_mtime {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_local(path: &str, hash: &str, size: i64) -> LocalFileInfo {
        LocalFileInfo {
            path: path.to_string(),
            hash: hash.to_string(),
            mtime: 1000,
            size,
        }
    }

    fn make_remote(path: &str, etag: &str, size: i64) -> RemoteFileInfo {
        RemoteFileInfo {
            path: path.to_string(),
            etag: Some(etag.to_string()),
            mtime: Some(1000),
            size: Some(size),
        }
    }

    fn make_known(path: &str, hash: &str, etag: &str, size: i64) -> FileState {
        FileState {
            path: path.to_string(),
            sync_zone: "personal".to_string(),
            local_hash: Some(hash.to_string()),
            local_mtime: Some(1000),
            local_size: Some(size),
            local_exists: true,
            remote_etag: Some(etag.to_string()),
            remote_mtime: Some(1000),
            remote_size: Some(size),
            remote_exists: true,
            sync_status: "synced".to_string(),
            last_synced_hash: Some(hash.to_string()),
            last_synced_mtime: Some(1000),
            last_synced_etag: Some(etag.to_string()),
            last_synced_at: Some("2026-01-01T00:00:00Z".to_string()),
            error_message: None,
            retry_count: 0,
        }
    }

    #[test]
    fn new_local_file_uploads() {
        let local = vec![make_local("doc.txt", "abc", 100)];
        let remote: Vec<RemoteFileInfo> = vec![];
        let known: Vec<FileState> = vec![];
        let result = compute_diff(&local, &remote, &known, "personal", "/local", "/remote");
        assert_eq!(result.len(), 1);
        assert!(matches!(result[0].action, SyncAction::Upload { .. }));
    }

    #[test]
    fn new_remote_file_downloads() {
        let local: Vec<LocalFileInfo> = vec![];
        let remote = vec![make_remote("doc.txt", "etag1", 100)];
        let known: Vec<FileState> = vec![];
        let result = compute_diff(&local, &remote, &known, "personal", "/local", "/remote");
        assert_eq!(result.len(), 1);
        assert!(matches!(result[0].action, SyncAction::Download { .. }));
    }

    #[test]
    fn synced_file_unchanged_skips() {
        let local = vec![make_local("doc.txt", "abc", 100)];
        let remote = vec![make_remote("doc.txt", "etag1", 100)];
        let known = vec![make_known("doc.txt", "abc", "etag1", 100)];
        let result = compute_diff(&local, &remote, &known, "personal", "/local", "/remote");
        assert!(result.is_empty()); // Skip filtered out
    }

    #[test]
    fn local_modified_uploads() {
        let local = vec![make_local("doc.txt", "new_hash", 200)];
        let remote = vec![make_remote("doc.txt", "etag1", 100)];
        let known = vec![make_known("doc.txt", "abc", "etag1", 100)];
        let result = compute_diff(&local, &remote, &known, "personal", "/local", "/remote");
        assert_eq!(result.len(), 1);
        assert!(matches!(result[0].action, SyncAction::Upload { .. }));
    }

    #[test]
    fn remote_modified_downloads() {
        let local = vec![make_local("doc.txt", "abc", 100)];
        let remote = vec![make_remote("doc.txt", "new_etag", 200)];
        let known = vec![make_known("doc.txt", "abc", "etag1", 100)];
        let result = compute_diff(&local, &remote, &known, "personal", "/local", "/remote");
        assert_eq!(result.len(), 1);
        assert!(matches!(result[0].action, SyncAction::Download { .. }));
    }

    #[test]
    fn both_modified_conflicts() {
        let local = vec![make_local("doc.txt", "new_hash", 200)];
        let remote = vec![make_remote("doc.txt", "new_etag", 300)];
        let known = vec![make_known("doc.txt", "abc", "etag1", 100)];
        let result = compute_diff(&local, &remote, &known, "personal", "/local", "/remote");
        assert_eq!(result.len(), 1);
        assert!(matches!(
            result[0].action,
            SyncAction::Conflict {
                conflict_type: ConflictType::BothModified,
                ..
            }
        ));
    }

    #[test]
    fn remote_deleted_unchanged_local_deletes_local() {
        let local = vec![make_local("doc.txt", "abc", 100)];
        let remote: Vec<RemoteFileInfo> = vec![];
        let known = vec![make_known("doc.txt", "abc", "etag1", 100)];
        let result = compute_diff(&local, &remote, &known, "personal", "/local", "/remote");
        assert_eq!(result.len(), 1);
        assert!(matches!(result[0].action, SyncAction::DeleteLocal { .. }));
    }

    #[test]
    fn local_deleted_unchanged_remote_deletes_remote() {
        let local: Vec<LocalFileInfo> = vec![];
        let remote = vec![make_remote("doc.txt", "etag1", 100)];
        let known = vec![make_known("doc.txt", "abc", "etag1", 100)];
        let result = compute_diff(&local, &remote, &known, "personal", "/local", "/remote");
        assert_eq!(result.len(), 1);
        assert!(matches!(result[0].action, SyncAction::DeleteRemote { .. }));
    }

    #[test]
    fn both_deleted_skips() {
        let local: Vec<LocalFileInfo> = vec![];
        let remote: Vec<RemoteFileInfo> = vec![];
        let known = vec![make_known("doc.txt", "abc", "etag1", 100)];
        let result = compute_diff(&local, &remote, &known, "personal", "/local", "/remote");
        assert!(result.is_empty());
    }
}
