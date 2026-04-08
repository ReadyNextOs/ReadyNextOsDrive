import { useState, useEffect, useCallback, FormEvent } from 'react';
import { getConfig, updateConfig, logout, pickFolder, type AppConfig } from '@/lib/tauri';

interface SettingsPageProps {
  onLogout: () => void;
}

export default function SettingsPage({ onLogout }: SettingsPageProps) {
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState('');
  const [loadError, setLoadError] = useState('');

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
    <div className="container">
      <form onSubmit={handleSave}>
        <div className="card">
          <h3 style={{ fontSize: 14, fontWeight: 600, marginBottom: 12 }}>Synchronizacja</h3>

          <div className="input-group">
            <label htmlFor="interval">Interwał synchronizacji (sekundy)</label>
            <input
              id="interval"
              type="number"
              className="input"
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

          <div className="input-group">
            <label>
              <input
                type="checkbox"
                checked={config.watch_local_changes}
                onChange={(e) => setConfig({ ...config, watch_local_changes: e.target.checked })}
                style={{ marginRight: 6 }}
              />
              Synchronizuj natychmiast po zmianie pliku
            </label>
          </div>

          <div className="input-group">
            <label>
              <input
                type="checkbox"
                checked={config.sync_on_startup}
                onChange={(e) => setConfig({ ...config, sync_on_startup: e.target.checked })}
                style={{ marginRight: 6 }}
              />
              Synchronizuj przy uruchomieniu
            </label>
          </div>
        </div>

        <div className="card">
          <h3 style={{ fontSize: 14, fontWeight: 600, marginBottom: 12 }}>Ścieżki</h3>

          <div className="input-group">
            <label htmlFor="personal-path">Moje pliki</label>
            <div style={{ display: 'flex', gap: 6 }}>
              <input
                id="personal-path"
                type="text"
                className="input"
                value={config.personal_sync_path}
                onChange={(e) => setConfig({ ...config, personal_sync_path: e.target.value })}
                style={{ flex: 1 }}
              />
              <button
                type="button"
                className="btn btn-outline"
                style={{ width: 'auto', padding: '8px 12px', whiteSpace: 'nowrap' }}
                onClick={async () => {
                  const folder = await pickFolder();
                  if (folder) setConfig({ ...config, personal_sync_path: folder });
                }}
              >
                Wybierz...
              </button>
            </div>
          </div>

          <div className="input-group">
            <label htmlFor="shared-path">Udostępnione</label>
            <div style={{ display: 'flex', gap: 6 }}>
              <input
                id="shared-path"
                type="text"
                className="input"
                value={config.shared_sync_path}
                onChange={(e) => setConfig({ ...config, shared_sync_path: e.target.value })}
                style={{ flex: 1 }}
              />
              <button
                type="button"
                className="btn btn-outline"
                style={{ width: 'auto', padding: '8px 12px', whiteSpace: 'nowrap' }}
                onClick={async () => {
                  const folder = await pickFolder();
                  if (folder) setConfig({ ...config, shared_sync_path: folder });
                }}
              >
                Wybierz...
              </button>
            </div>
          </div>
        </div>

        <div className="card">
          <h3 style={{ fontSize: 14, fontWeight: 600, marginBottom: 12 }}>Konto</h3>
          <p style={{ fontSize: 12, color: 'var(--color-text-secondary)', marginBottom: 12 }}>
            {config.user_email}<br />
            Serwer: {config.server_url}
          </p>

          {message && (
            <p style={{ fontSize: 12, marginBottom: 8, color: message.startsWith('Błąd') ? 'var(--color-error)' : 'var(--color-success)' }}>
              {message}
            </p>
          )}

          <button
            type="submit"
            className="btn btn-primary"
            disabled={saving}
          >
            {saving ? 'Zapisywanie...' : 'Zapisz ustawienia'}
          </button>

          <hr className="section-separator" />

          <button
            type="button"
            className="btn btn-danger"
            onClick={handleLogout}
          >
            Wyloguj
          </button>
        </div>
      </form>
    </div>
  );
}
