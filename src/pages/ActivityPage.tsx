import { useState, useEffect, useCallback } from 'react';
import {
  getActivity,
  getSyncHistory,
  type ActivityEntry,
  type SyncRunEntry,
} from '@/lib/tauri';

type Tab = 'current' | 'history';

export default function ActivityPage() {
  const [tab, setTab] = useState<Tab>('current');
  const [entries, setEntries] = useState<ActivityEntry[]>([]);
  const [history, setHistory] = useState<SyncRunEntry[]>([]);
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

  // Load sync history when switching to history tab
  useEffect(() => {
    if (tab !== 'history') return;
    let cancelled = false;
    getSyncHistory(100)
      .then((data) => {
        if (!cancelled) setHistory(data);
      })
      .catch((err) => {
        if (!cancelled) console.error('Failed to load sync history:', err);
      });
    return () => {
      cancelled = true;
    };
  }, [tab]);

  const exportHistoryCsv = useCallback(() => {
    const header =
      'id,started_at,completed_at,status,source,uploaded,downloaded,deleted,conflicted,bytes,duration_ms,error';
    const rows = history.map((r) =>
      [
        r.id,
        r.started_at,
        r.completed_at ?? '',
        r.status,
        r.source ?? '',
        r.files_uploaded,
        r.files_downloaded,
        r.files_deleted,
        r.files_conflicted,
        r.bytes_transferred,
        r.duration_ms ?? '',
        `"${(r.error_message ?? '').replace(/"/g, '""')}"`,
      ].join(',')
    );
    const csv = [header, ...rows].join('\n');
    const blob = new Blob([csv], { type: 'text/csv;charset=utf-8;' });
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.href = url;
    link.download = `sync-history-${new Date().toISOString().slice(0, 10)}.csv`;
    link.click();
    URL.revokeObjectURL(url);
  }, [history]);

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
      <div style={{ display: 'flex', gap: 4, marginBottom: 8 }}>
        <button
          type="button"
          className={`btn btn-sm ${tab === 'current' ? 'btn-primary' : 'btn-outline'}`}
          onClick={() => setTab('current')}
          style={{ flex: 1 }}
        >
          Bieżące
        </button>
        <button
          type="button"
          className={`btn btn-sm ${tab === 'history' ? 'btn-primary' : 'btn-outline'}`}
          onClick={() => setTab('history')}
          style={{ flex: 1 }}
        >
          Historia
        </button>
      </div>

      {tab === 'current' && (
        <div className="card">
          <h3 style={{ fontSize: 14, fontWeight: 600, marginBottom: 8 }}>Ostatnia aktywność</h3>

          {entries.length === 0 && (
            <p style={{ fontSize: 12, color: 'var(--color-text-secondary)' }}>Brak aktywności.</p>
          )}

          {entries.map((entry) => (
            <div className="activity-item" key={getEntryKey(entry)}>
              <div style={{ display: 'flex', justifyContent: 'space-between' }}>
                <span style={{ fontWeight: 500 }}>{formatAction(entry.action)}</span>
                <span
                  className={`status-badge status-${entry.status === 'success' ? 'idle' : 'error'}`}
                >
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
                  <span className="activity-error-summary">{getErrorSummary(entry)}</span>
                  {entry.details && (
                    <button
                      type="button"
                      className="icon-button"
                      onClick={() => void handleCopyError(entry)}
                      title="Kopiuj szczegóły błędu"
                      aria-label="Kopiuj szczegóły błędu"
                    >
                      <ClipboardIcon />
                      <span>
                        {copiedKey === getEntryKey(entry) ? 'Skopiowano' : 'Kopiuj błąd'}
                      </span>
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
      )}

      {tab === 'history' && (
        <div className="card">
          <div
            style={{
              display: 'flex',
              justifyContent: 'space-between',
              alignItems: 'center',
              marginBottom: 8,
            }}
          >
            <h3 style={{ fontSize: 14, fontWeight: 600 }}>Historia synchronizacji</h3>
            <button
              type="button"
              className="btn btn-outline btn-sm"
              onClick={exportHistoryCsv}
              disabled={history.length === 0}
            >
              Eksport CSV
            </button>
          </div>

          {history.length === 0 && (
            <p style={{ fontSize: 12, color: 'var(--color-text-secondary)' }}>
              Brak historii synchronizacji.
            </p>
          )}

          {history.map((run) => (
            <div className="activity-item" key={run.id}>
              <div style={{ display: 'flex', justifyContent: 'space-between' }}>
                <span style={{ fontWeight: 500 }}>
                  {formatSource(run.source)} &middot; {formatStatusLabel(run.status)}
                </span>
                <span className={`status-badge status-${getHistoryStatusClass(run.status)}`}>
                  {run.status.toUpperCase()}
                </span>
              </div>
              <div style={{ fontSize: 11, color: 'var(--color-text-secondary)', marginTop: 2 }}>
                ↑ {run.files_uploaded} &middot; ↓ {run.files_downloaded} &middot; ✕{' '}
                {run.files_deleted}
                {run.files_conflicted > 0 && ` · ⚠ ${run.files_conflicted}`}
                {run.bytes_transferred > 0 && ` · ${formatBytes(run.bytes_transferred)}`}
                {run.duration_ms != null && ` · ${formatDuration(run.duration_ms)}`}
              </div>
              {run.error_message && (
                <div className="activity-error-row" style={{ marginTop: 4 }}>
                  <span className="activity-error-summary">{run.error_message}</span>
                </div>
              )}
              <div className="activity-time">
                {new Date(run.started_at).toLocaleString('pl-PL')}
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function formatSource(source: string | null): string {
  switch (source) {
    case 'startup':
      return 'Start';
    case 'interval':
      return 'Interwał';
    case 'watcher':
      return 'Watcher';
    case 'manual':
      return 'Ręczna';
    default:
      return source ?? '—';
  }
}

function formatStatusLabel(status: string): string {
  switch (status) {
    case 'success':
      return 'Zakończono';
    case 'error':
      return 'Błąd';
    case 'partial':
      return 'Częściowo';
    case 'running':
      return 'W toku';
    default:
      return status;
  }
}

function getHistoryStatusClass(status: string): string {
  if (status === 'success') return 'idle';
  if (status === 'error') return 'error';
  if (status === 'partial') return 'conflict';
  return 'syncing';
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms} ms`;
  const s = ms / 1000;
  if (s < 60) return `${s.toFixed(1)} s`;
  const m = Math.floor(s / 60);
  return `${m} min ${Math.floor(s % 60)} s`;
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
