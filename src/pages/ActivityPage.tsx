import { useState, useEffect } from 'react';
import { getActivity, type ActivityEntry } from '@/lib/tauri';

export default function ActivityPage() {
  const [entries, setEntries] = useState<ActivityEntry[]>([]);

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
          <div className="activity-item" key={`${entry.timestamp}-${entry.action}-${entry.file_path}`}>
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
            {entry.details && (
              <div style={{ color: 'var(--color-error)', fontSize: 11, marginTop: 2 }}>
                {entry.details}
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
