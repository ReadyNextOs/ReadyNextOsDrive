import { useState, useEffect } from 'react';
import { getActivity, type ActivityEntry } from '@/lib/tauri';

export default function ActivityPage() {
  const [entries, setEntries] = useState<ActivityEntry[]>([]);
  const [copiedKey, setCopiedKey] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    let inFlight = false;

    const refresh = async () => {
      if (inFlight) return;
      inFlight = true;
      try {
        const data = await getActivity(100);
        if (!cancelled) {
          setEntries([...data].reverse());
        }
      } catch (err) {
        if (!cancelled) {
          console.error('Failed to load activity:', err);
        }
      } finally {
        inFlight = false;
      }
    };

    refresh();
    const interval = setInterval(refresh, 10000);
    return () => {
      cancelled = true;
      clearInterval(interval);
    };
  }, []);

  useEffect(() => {
    if (!copiedKey) {
      return undefined;
    }

    const timeout = window.setTimeout(() => {
      setCopiedKey(null);
    }, 2000);

    return () => window.clearTimeout(timeout);
  }, [copiedKey]);

  const handleCopyError = async (entry: ActivityEntry) => {
    if (!entry.details) {
      return;
    }

    const key = getEntryKey(entry);
    try {
      await navigator.clipboard.writeText(entry.details);
      setCopiedKey(key);
    } catch (err) {
      console.error('Failed to copy error details:', err);
    }
  };

  return (
    <div className="container">
      <div className="card">
        <h3 style={{ fontSize: 14, fontWeight: 600, marginBottom: 8 }}>
          Ostatnia aktywność
        </h3>

        {entries.length === 0 && (
          <p style={{ fontSize: 12, color: 'var(--color-text-secondary)' }}>
            Brak aktywności.
          </p>
        )}

        {entries.map((entry) => (
          <div className="activity-item" key={getEntryKey(entry)}>
            <div style={{ display: 'flex', justifyContent: 'space-between' }}>
              <span style={{ fontWeight: 500 }}>
                {formatAction(entry.action)}
              </span>
              <span className={`status-badge status-${entry.status === 'success' ? 'idle' : 'error'}`}>
                {entry.status === 'success' ? 'OK' : 'Błąd'}
              </span>
            </div>
            {entry.file_path && (
              <div className="activity-path" title={entry.file_path}>
                {entry.file_path}
              </div>
            )}
            {entry.status !== 'success' && (
              <div className="activity-error-row">
                <span className="activity-error-summary">
                  {getErrorSummary(entry)}
                </span>
                {entry.details && (
                  <button
                    type="button"
                    className="icon-button"
                    onClick={() => void handleCopyError(entry)}
                    title="Kopiuj szczegóły błędu"
                    aria-label="Kopiuj szczegóły błędu"
                  >
                    <ClipboardIcon />
                    <span>{copiedKey === getEntryKey(entry) ? 'Skopiowano' : 'Kopiuj błąd'}</span>
                  </button>
                )}
              </div>
            )}
            <div className="activity-time">
              {new Date(entry.timestamp).toLocaleString('pl-PL')}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

function getErrorSummary(entry: ActivityEntry): string {
  if (entry.status === 'success') {
    return '';
  }

  return `Błąd: ${formatAction(entry.action)}`;
}

function getEntryKey(entry: ActivityEntry): string {
  return `${entry.timestamp}-${entry.action}-${entry.file_path}`;
}

function ClipboardIcon() {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path
        d="M16 4h-1.18A3 3 0 0 0 12 2a3 3 0 0 0-2.82 2H8a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h8a2 2 0 0 0 2-2V6a2 2 0 0 0-2-2Zm-4-1a1 1 0 0 1 .96.73l.1.27H10.94l.1-.27A1 1 0 0 1 12 3Zm4 15H8V6h1v1h6V6h1v12Z"
        fill="currentColor"
      />
    </svg>
  );
}

function formatAction(action: string): string {
  switch (action) {
    case 'sync_personal': return 'Sync: Moje pliki';
    case 'sync_shared': return 'Sync: Udostępnione';
    case 'upload': return 'Upload';
    case 'download': return 'Download';
    case 'delete': return 'Usunięto';
    case 'conflict': return 'Konflikt';
    default: return action;
  }
}
