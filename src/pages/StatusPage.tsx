import { useState, useEffect, useCallback } from 'react';
import { getSyncStatus, getConfig, triggerSync, openFolder, type SyncStatus, type AppConfig } from '@/lib/tauri';

export default function StatusPage() {
  const [status, setStatus] = useState<SyncStatus>('NotConfigured');
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [syncing, setSyncing] = useState(false);

  const refreshStatus = useCallback(async () => {
    try {
      const [s, c] = await Promise.all([getSyncStatus(), getConfig()]);
      setStatus(s);
      setConfig(c);
    } catch (err) {
      console.error('Failed to refresh status:', err);
    }
  }, []);

  useEffect(() => {
    refreshStatus();
    const interval = setInterval(refreshStatus, 5000);
    return () => clearInterval(interval);
  }, [refreshStatus]);

  const handleSync = useCallback(async () => {
    setSyncing(true);
    try {
      await triggerSync();
    } catch (err) {
      console.error('Sync failed:', err);
    } finally {
      setSyncing(false);
      refreshStatus();
    }
  }, [refreshStatus]);

  const handleOpenPersonal = useCallback(() => {
    if (config) openFolder(config.personal_sync_path);
  }, [config]);

  const handleOpenShared = useCallback(() => {
    if (config) openFolder(config.shared_sync_path);
  }, [config]);

  const statusLabel = getStatusLabel(status);
  const statusClass = getStatusClass(status);

  return (
    <div className="container">
      <div className="card">
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 12 }}>
          <h3 style={{ fontSize: 14, fontWeight: 600 }}>Status synchronizacji</h3>
          <span className={`status-badge ${statusClass}`}>{statusLabel}</span>
        </div>

        {config && (
          <p style={{ fontSize: 12, color: 'var(--color-text-secondary)', marginBottom: 12 }}>
            {config.user_email} &middot; {config.server_url}
          </p>
        )}

        <button
          className="btn btn-primary"
          onClick={handleSync}
          disabled={syncing}
        >
          {syncing ? 'Synchronizacja...' : 'Synchronizuj teraz'}
        </button>
      </div>

      <div className="card">
        <h3 style={{ fontSize: 14, fontWeight: 600, marginBottom: 8 }}>Foldery</h3>

        <div className="folder-link" onClick={handleOpenPersonal}>
          <span style={{ fontSize: 18 }}>📁</span>
          <div>
            <div style={{ fontWeight: 500 }}>Moje pliki</div>
            <div style={{ fontSize: 11, color: 'var(--color-text-secondary)' }}>
              {config?.personal_sync_path}
            </div>
          </div>
        </div>

        <div className="folder-link" onClick={handleOpenShared}>
          <span style={{ fontSize: 18 }}>📂</span>
          <div>
            <div style={{ fontWeight: 500 }}>Udostępnione</div>
            <div style={{ fontSize: 11, color: 'var(--color-text-secondary)' }}>
              {config?.shared_sync_path}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

function getStatusLabel(status: SyncStatus): string {
  if (status === 'Idle') return 'Zsynchronizowane';
  if (status === 'Syncing') return 'Synchronizacja...';
  if (status === 'Conflict') return 'Konflikt';
  if (status === 'NotConfigured') return 'Nie skonfigurowano';
  if (typeof status === 'object' && 'Error' in status) return 'Błąd';
  return 'Nieznany';
}

function getStatusClass(status: SyncStatus): string {
  if (status === 'Idle') return 'status-idle';
  if (status === 'Syncing') return 'status-syncing';
  if (status === 'Conflict') return 'status-conflict';
  if (typeof status === 'object' && 'Error' in status) return 'status-error';
  return '';
}
