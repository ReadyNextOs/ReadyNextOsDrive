import { invoke } from '@tauri-apps/api/core';

export interface AppConfig {
  server_url: string;
  user_email: string;
  tenant_id: string;
  personal_sync_path: string;
  shared_sync_path: string;
  sync_interval_secs: number;
  watch_local_changes: boolean;
  sync_on_startup: boolean;
  max_file_size_bytes: number;
}

export type SyncStatus =
  | 'Idle'
  | 'Syncing'
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

export interface LoginUser {
  id: string;
  email: string;
  name: string;
  tenant_id: string;
}

export async function login(serverUrl: string, email: string, password: string): Promise<LoginUser> {
  const result = await invoke<string>('login', { serverUrl, email, password });
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

export async function openFolder(path: string): Promise<void> {
  await invoke('open_folder', { path });
}
