import { useState, useEffect, useCallback, FormEvent } from 'react';
import { getConfig, updateConfig, logout, pickFolder, getDebugInfo, setDebugMode, getLogContents, type AppConfig } from '@/lib/tauri';

interface SettingsPageProps {
  onLogout: () => void;
}

export default function SettingsPage({ onLogout }: SettingsPageProps) {
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState('');
  const [loadError, setLoadError] = useState('');
  const [debugEnabled, setDebugEnabled] = useState(false);
  const [logPath, setLogPath] = useState('');
  const [logContents, setLogContents] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    getConfig()
      .then((nextConfig) => {
        if (!cancelled) {
          setConfig(nextConfig);
          setLoadError('');
        }
      })
      .catch((err) => {
        if (!cancelled) {
          setLoadError(`Nie udało się wczytać ustawień: ${err}`);
        }
      });

    getDebugInfo()
      .then(([enabled, path]) => {
        if (!cancelled) {
          setDebugEnabled(enabled);
          setLogPath(path);
        }
      })
      .catch(() => {});

    return () => {
      cancelled = true;
    };
  }, []);

  const handleSave = useCallback(async (e: FormEvent) => {
    e.preventDefault();
    if (!config) return;

    setSaving(true);
    setMessage('');
    try {
      await updateConfig(config);
      setMessage('Ustawienia zapisane');
    } catch (err) {
      setMessage(`Błąd: ${err}`);
    } finally {
      setSaving(false);
    }
  }, [config]);

  const handleLogout = useCallback(async () => {
    const confirmed = window.confirm('Czy na pewno chcesz się wylogować? Synchronizacja zostanie zatrzymana.');
    if (!confirmed) return;

    try {
      await logout();
      onLogout();
    } catch (err) {
      console.error('Logout failed:', err);
    }
  }, [onLogout]);

  if (loadError) {
    return (
      <div className="container">
        <div className="card">
          <p className="error">{loadError}</p>
        </div>
      </div>
    );
  }

  if (!config) {
    return (
      <div className="container">
        <div className="card">
          <p style={{ fontSize: 12, color: 'var(--color-text-secondary)' }}>Ładowanie ustawień...</p>
        </div>
      </div>
    );
  }

  return (
    <div className="container settings-compact">
      <form onSubmit={handleSave}>
        <div className="card">
          <h3 className="card-title">Synchronizacja</h3>

          <div className="input-group">
            <label htmlFor="interval">Interwał (sekundy)</label>
            <input
              id="interval"
              type="number"
              className="input input-sm"
              min={30}
              max={3600}
              value={config.sync_interval_secs}
              onChange={(e) => {
                const nextValue = parseInt(e.target.value, 10);
                if (!Number.isNaN(nextValue)) {
                  setConfig({
                    ...config,
                    sync_interval_secs: Math.max(30, Math.min(3600, nextValue)),
                  });
                }
              }}
            />
          </div>

          <label className="checkbox-row">
            <input
              type="checkbox"
              checked={config.watch_local_changes}
              onChange={(e) => setConfig({ ...config, watch_local_changes: e.target.checked })}
            />
            Synchronizuj po zmianie pliku
          </label>

          <label className="checkbox-row">
            <input
              type="checkbox"
              checked={config.sync_on_startup}
              onChange={(e) => setConfig({ ...config, sync_on_startup: e.target.checked })}
            />
            Synchronizuj przy uruchomieniu
          </label>
        </div>

        <div className="card">
          <h3 className="card-title">Ścieżki</h3>

          <div className="input-group">
            <label htmlFor="personal-path">Moje pliki</label>
            <div className="path-row">
              <input
                id="personal-path"
                type="text"
                className="input input-sm"
                value={config.personal_sync_path}
                onChange={(e) => setConfig({ ...config, personal_sync_path: e.target.value })}
                style={{ flex: 1 }}
              />
              <button
                type="button"
                className="btn btn-outline btn-sm"
                onClick={async () => {
                  const folder = await pickFolder();
                  if (folder) setConfig({ ...config, personal_sync_path: folder });
                }}
              >
                ...
              </button>
            </div>
          </div>

          <div className="input-group">
            <label htmlFor="shared-path">Udostępnione</label>
            <div className="path-row">
              <input
                id="shared-path"
                type="text"
                className="input input-sm"
                value={config.shared_sync_path}
                onChange={(e) => setConfig({ ...config, shared_sync_path: e.target.value })}
                style={{ flex: 1 }}
              />
              <button
                type="button"
                className="btn btn-outline btn-sm"
                onClick={async () => {
                  const folder = await pickFolder();
                  if (folder) setConfig({ ...config, shared_sync_path: folder });
                }}
              >
                ...
              </button>
            </div>
          </div>
        </div>

        <div className="card">
          <h3 className="card-title">Konto</h3>
          <p style={{ fontSize: 11, color: 'var(--color-text-secondary)', marginBottom: 8 }}>
            {config.user_email} &middot; {config.server_url}
          </p>

          {message && (
            <p style={{ fontSize: 11, marginBottom: 6, color: message.startsWith('Błąd') ? 'var(--color-error)' : 'var(--color-success)' }}>
              {message}
            </p>
          )}

          <div style={{ display: 'flex', gap: 8 }}>
            <button type="submit" className="btn btn-primary btn-sm" disabled={saving} style={{ flex: 1 }}>
              {saving ? 'Zapisywanie...' : 'Zapisz'}
            </button>
            <button type="button" className="btn btn-danger btn-sm" onClick={handleLogout} style={{ flex: 1 }}>
              Wyloguj
            </button>
          </div>
        </div>
        <div className="card">
          <h3 className="card-title">Diagnostyka</h3>

          <label className="checkbox-row">
            <input
              type="checkbox"
              checked={debugEnabled}
              onChange={async (e) => {
                const next = e.target.checked;
                await setDebugMode(next);
                setDebugEnabled(next);
              }}
            />
            Tryb debug (szczegółowe logi)
          </label>

          <p style={{ fontSize: 10, color: 'var(--color-text-secondary)', margin: '4px 0 8px', wordBreak: 'break-all' }}>
            Logi: {logPath}
          </p>

          <button
            type="button"
            className="btn btn-outline btn-sm"
            onClick={async () => {
              try {
                const contents = await getLogContents(100);
                setLogContents(contents);
              } catch (err) {
                setLogContents(`Błąd: ${err}`);
              }
            }}
          >
            Pokaż logi
          </button>

          {logContents !== null && (
            <pre style={{
              fontSize: 10,
              marginTop: 8,
              padding: 8,
              background: '#f5f5f5',
              borderRadius: 4,
              maxHeight: 200,
              overflow: 'auto',
              whiteSpace: 'pre-wrap',
              wordBreak: 'break-all',
            }}>
              {logContents}
            </pre>
          )}
        </div>
      </form>
    </div>
  );
}
