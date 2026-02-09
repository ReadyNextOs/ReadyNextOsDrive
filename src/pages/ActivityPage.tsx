import { useState, useEffect, useCallback } from 'react';
import { getActivity, type ActivityEntry } from '@/lib/tauri';

export default function ActivityPage() {
  const [entries, setEntries] = useState<ActivityEntry[]>([]);

  const refresh = useCallback(async () => {
    try {
      const data = await getActivity(100);
      setEntries([...data].reverse());
    } catch (err) {
      console.error('Failed to load activity:', err);
    }
  }, []);

  useEffect(() => {
    refresh();
    const interval = setInterval(refresh, 10000);
    return () => clearInterval(interval);
  }, [refresh]);

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

        {entries.map((entry, i) => (
          <div className="activity-item" key={i}>
            <div style={{ display: 'flex', justifyContent: 'space-between' }}>
              <span style={{ fontWeight: 500 }}>
                {formatAction(entry.action)}
              </span>
              <span className={`status-badge status-${entry.status === 'success' ? 'idle' : 'error'}`}>
                {entry.status === 'success' ? 'OK' : 'Błąd'}
              </span>
            </div>
            {entry.file_path && (
              <div style={{ color: 'var(--color-text-secondary)', fontSize: 11 }}>
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
