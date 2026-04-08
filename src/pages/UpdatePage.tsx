import { useState, useEffect, useCallback } from 'react';
import { check, type Update } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import { getVersion } from '@tauri-apps/api/app';

export default function UpdatePage() {
  const [currentVersion, setCurrentVersion] = useState('');
  const [checking, setChecking] = useState(false);
  const [update, setUpdate] = useState<Update | null>(null);
  const [noUpdate, setNoUpdate] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [downloading, setDownloading] = useState(false);
  const [progress, setProgress] = useState(0);
  const [installed, setInstalled] = useState(false);

  useEffect(() => {
    getVersion().then(setCurrentVersion).catch(() => {});
  }, []);

  const handleCheck = useCallback(async () => {
    setChecking(true);
    setError(null);
    setNoUpdate(false);
    setUpdate(null);
    try {
      const result = await check();
      if (result) {
        setUpdate(result);
      } else {
        setNoUpdate(true);
      }
    } catch (err) {
      setError(String(err).replace(/^(Error|invoke error):\s*/gi, '').trim());
    } finally {
      setChecking(false);
    }
  }, []);

  const handleUpdate = useCallback(async () => {
    if (!update) return;
    setDownloading(true);
    setError(null);
    setProgress(0);
    try {
      let downloaded = 0;
      await update.downloadAndInstall((event) => {
        if (event.event === 'Started' && event.data.contentLength) {
          setProgress(0);
        } else if (event.event === 'Progress') {
          downloaded += event.data.chunkLength;
          setProgress(downloaded);
        } else if (event.event === 'Finished') {
          setInstalled(true);
        }
      });
      setInstalled(true);
    } catch (err) {
      setError(String(err).replace(/^(Error|invoke error):\s*/gi, '').trim());
      setDownloading(false);
    }
  }, [update]);

  const handleRelaunch = useCallback(async () => {
    await relaunch();
  }, []);

  return (
    <div className="container">
      <div className="card">
        <h3 className="card-title">Aktualizacja</h3>
        <p style={{ fontSize: 12, color: 'var(--color-text-secondary)', marginBottom: 12 }}>
          Aktualna wersja: <strong>v{currentVersion}</strong>
        </p>

        {!update && !noUpdate && !installed && (
          <button
            className="btn btn-primary btn-sm"
            onClick={handleCheck}
            disabled={checking}
          >
            {checking ? 'Sprawdzanie...' : 'Sprawdź aktualizacje'}
          </button>
        )}

        {noUpdate && (
          <p style={{ fontSize: 12, color: 'var(--color-success)', marginTop: 8 }}>
            Masz najnowszą wersję.
          </p>
        )}

        {error && (
          <p className="error-detail" style={{ marginTop: 8 }}>{error}</p>
        )}
      </div>

      {update && !installed && (
        <div className="card">
          <h3 className="card-title">Dostępna aktualizacja</h3>
          <p style={{ fontSize: 13, fontWeight: 500, marginBottom: 4 }}>
            v{update.version}
          </p>
          {update.body && (
            <p style={{ fontSize: 11, color: 'var(--color-text-secondary)', marginBottom: 12, whiteSpace: 'pre-wrap' }}>
              {update.body}
            </p>
          )}

          {downloading && (
            <div style={{ marginBottom: 8 }}>
              <div style={{
                height: 4,
                background: '#e0e0e0',
                borderRadius: 2,
                overflow: 'hidden',
              }}>
                <div style={{
                  height: '100%',
                  background: 'var(--color-primary)',
                  width: progress > 0 ? '100%' : '30%',
                  transition: 'width 0.3s',
                  animation: progress === 0 ? 'none' : undefined,
                }} />
              </div>
              <p style={{ fontSize: 10, color: 'var(--color-text-secondary)', marginTop: 4 }}>
                {progress > 0
                  ? `Pobrano ${(progress / 1024 / 1024).toFixed(1)} MB`
                  : 'Pobieranie...'}
              </p>
            </div>
          )}

          <button
            className="btn btn-primary btn-sm"
            onClick={handleUpdate}
            disabled={downloading}
          >
            {downloading ? 'Instalowanie...' : 'Aktualizuj'}
          </button>
        </div>
      )}

      {installed && (
        <div className="card">
          <h3 className="card-title" style={{ color: 'var(--color-success)' }}>Zainstalowano</h3>
          <p style={{ fontSize: 12, marginBottom: 12 }}>
            Aktualizacja została zainstalowana. Uruchom ponownie aplikację.
          </p>
          <button className="btn btn-primary btn-sm" onClick={handleRelaunch}>
            Uruchom ponownie
          </button>
        </div>
      )}
    </div>
  );
}
