import { invoke } from '@tauri-apps/api/core';

export interface AppConfig {
  server_url: string;
  user_login: string;
  tenant_id: string;
  personal_sync_path: string;
  shared_sync_path: string;
  sync_interval_secs: number;
  watch_local_changes: boolean;
  sync_on_startup: boolean;
  max_file_size_bytes: number;
  max_upload_kbps: number;
  max_download_kbps: number;
  sync_include_paths: string[];
}

export type SyncStatus =
  | 'Idle'
  | 'Syncing'
  | 'Paused'
  | 'Conflict'
  | { Error: string }
  | 'NotConfigured';

export interface ActivityEntry {
  timestamp: string;
  action: string;
  file_path: string;
  status: string;
  details: string | null;
}

export interface SyncRunEntry {
  id: number;
  started_at: string;
  completed_at: string | null;
  status: 'running' | 'success' | 'error' | 'partial';
  source: string | null;
  files_uploaded: number;
  files_downloaded: number;
  files_deleted: number;
  files_conflicted: number;
  bytes_transferred: number;
  error_message: string | null;
  duration_ms: number | null;
}

export interface SyncConflictPayload {
  path: string;
  localPath: string;
  remotePath: string;
  conflictType: 'BothModified' | 'DeletedLocallyModifiedRemotely' | 'DeletedRemotelyModifiedLocally';
  resolution: string;
}

export interface SyncFileProgressPayload {
  path: string;
  action: 'upload' | 'download' | 'delete';
  current: number;
  total: number;
}

export interface LoginUser {
  id: string;
  login: string;
  name: string;
  tenant_id: string;
}

export async function login(serverUrl: string, login: string, password: string): Promise<LoginUser> {
  const result = await invoke<string>('login', { serverUrl, login, password });
  return JSON.parse(result);
}

export async function loginWithToken(token: string): Promise<LoginUser> {
  const result = await invoke<string>('login_with_token', { token });
  return JSON.parse(result);
}

export async function logout(): Promise<void> {
  await invoke('logout');
}

export async function getSyncStatus(): Promise<SyncStatus> {
  return invoke<SyncStatus>('get_sync_status');
}

export async function getConfig(): Promise<AppConfig> {
  return invoke<AppConfig>('get_config');
}

export async function updateConfig(config: AppConfig): Promise<void> {
  await invoke('update_config', { config });
}

export async function triggerSync(): Promise<void> {
  await invoke('trigger_sync');
}

export async function getActivity(limit?: number): Promise<ActivityEntry[]> {
  return invoke<ActivityEntry[]>('get_activity', { limit: limit ?? 50 });
}

export async function getSyncHistory(limit?: number): Promise<SyncRunEntry[]> {
  return invoke<SyncRunEntry[]>('get_sync_history', { limit: limit ?? 50 });
}

export async function pauseSync(): Promise<void> {
  await invoke('pause_sync');
}

export async function resumeSync(): Promise<void> {
  await invoke('resume_sync');
}

export async function openFolder(path: string): Promise<void> {
  await invoke('open_folder', { path });
}

export async function pickFolder(): Promise<string | null> {
  return invoke<string | null>('pick_folder');
}

export async function getDebugInfo(): Promise<[boolean, string]> {
  return invoke<[boolean, string]>('get_debug_info');
}

export async function openLogFile(): Promise<void> {
  await invoke('open_log_file');
}

export async function setDebugMode(enabled: boolean): Promise<void> {
  await invoke('set_debug_mode', { enabled });
}

export async function getLogContents(maxLines?: number): Promise<string> {
  return invoke<string>('get_log_contents', { maxLines: maxLines ?? 200 });
}
