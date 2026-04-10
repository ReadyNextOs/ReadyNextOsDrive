import { useState, useEffect, useCallback } from 'react';
import { check, type Update } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import { getVersion } from '@tauri-apps/api/app';

/**
 * Translate cryptic errors from tauri-plugin-updater into a Polish, user-
 * readable message. Covers the known failure modes we've hit in production.
 */
function friendlyUpdateError(raw: unknown): string {
  const msg = String(raw).replace(/^(Error|invoke error):\s*/gi, '').trim();
  const lower = msg.toLowerCase();

  // Race window on release day: build has created the GitHub release but the
  // finalize job hasn't uploaded latest.json yet, so /latest/download/latest.json
  // redirects to a tag that doesn't have the manifest.
  if (
    lower.includes('could not fetch a valid release json') ||
    lower.includes('not found')
  ) {
    return 'Nowa wersja jest właśnie przygotowywana na serwerze. Spróbuj ponownie za 2–3 minuty.';
  }

  // Repacked AppImage vs stale .sig (fixed in 0.6.1 CI, but keep message).
  if (lower.includes('signature verification failed')) {
    return 'Podpis nowej wersji nie zgadza się z plikiem. Skontaktuj się z administratorem lub spróbuj pobrać aktualizację ręcznie.';
  }

  // Generic network failures.
  if (
    lower.includes('network') ||
    lower.includes('timeout') ||
    lower.includes('dns') ||
    lower.includes('connection') ||
    lower.includes('tls') ||
    lower.includes('certificate')
  ) {
    return 'Brak połączenia z serwerem aktualizacji. Sprawdź internet i spróbuj ponownie.';
  }

  // No signature or bad update manifest structure.
  if (lower.includes('failed to deserialize') || lower.includes('missing field')) {
    return 'Plik z informacją o aktualizacji jest uszkodzony. Spróbuj ponownie za chwilę.';
  }

  // Fallback — show the raw message so we can still diagnose unknown issues.
  return `Nie udało się sprawdzić aktualizacji: ${msg}`;
}

export default function UpdatePage() {
  const [currentVersion, setCurrentVersion] = useState('');
  const [checking, setChecking] = useState(false);
  const [update, setUpdate] = useState<Update | null>(null);
  const [noUpdate, setNoUpdate] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [downloading, setDownloading] = useState(false);
  const [progress, setProgress] = useState(0);
  const [total, setTotal] = useState<number | null>(null);
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
      setError(friendlyUpdateError(err));
    } finally {
      setChecking(false);
    }
  }, []);

  const handleUpdate = useCallback(async () => {
    if (!update) return;
    setDownloading(true);
    setError(null);
    setProgress(0);
    setTotal(null);
    try {
      let downloaded = 0;
      await update.downloadAndInstall((event) => {
        if (event.event === 'Started') {
          downloaded = 0;
          setProgress(0);
          setTotal(event.data.contentLength ?? null);
        } else if (event.event === 'Progress') {
          downloaded += event.data.chunkLength;
          setProgress(downloaded);
        } else if (event.event === 'Finished') {
          setInstalled(true);
        }
      });
      setInstalled(true);
    } catch (err) {
      setError(friendlyUpdateError(err));
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

          {downloading && (() => {
            const mb = (bytes: number) => (bytes / 1024 / 1024).toFixed(1);
            const pct = total && total > 0 ? Math.min(100, (progress / total) * 100) : null;
            return (
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
                    width: pct !== null ? `${pct}%` : '30%',
                    transition: 'width 0.2s linear',
                  }} />
                </div>
                <p style={{ fontSize: 10, color: 'var(--color-text-secondary)', marginTop: 4 }}>
                  {pct !== null
                    ? `${mb(progress)} / ${mb(total!)} MB (${pct.toFixed(0)}%)`
                    : progress > 0
                      ? `Pobrano ${mb(progress)} MB`
                      : 'Pobieranie...'}
                </p>
              </div>
            );
          })()}

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
