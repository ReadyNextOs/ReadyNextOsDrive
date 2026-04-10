import { useState, useEffect, useCallback } from 'react';
import { listen } from '@tauri-apps/api/event';
import type { SyncConflictPayload } from '@/lib/tauri';

/**
 * ConflictDialog — modal that appears when the sync engine detects a conflict.
 *
 * Listens for `sync-conflict` events emitted by the Rust backend.
 * Currently the backend auto-skips conflicts (Phase 4 resolution pending),
 * so this dialog acts as a notification + queue UI. Once the backend adds
 * a `resolve_conflict` command, the resolution buttons will wire through.
 */
export default function ConflictDialog() {
  const [queue, setQueue] = useState<SyncConflictPayload[]>([]);
  const [dismissedPaths, setDismissedPaths] = useState<Set<string>>(new Set());

  useEffect(() => {
    const unlisten = listen<SyncConflictPayload>('sync-conflict', (event) => {
      const payload = event.payload;
      setQueue((prev) => {
        // Dedupe by path — only one active dialog per file
        if (prev.some((c) => c.path === payload.path)) return prev;
        return [...prev, payload];
      });
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const current = queue.find((c) => !dismissedPaths.has(c.path)) ?? null;

  const handleDismiss = useCallback(() => {
    if (!current) return;
    setDismissedPaths((prev) => new Set(prev).add(current.path));
  }, [current]);

  const handleResolve = useCallback(
    async (resolution: 'keep_local' | 'keep_remote' | 'keep_both' | 'skip') => {
      if (!current) return;
      // TODO: wire to backend resolve_conflict command when implemented
      console.log(`Conflict resolution: ${resolution} for ${current.path}`);
      handleDismiss();
    },
    [current, handleDismiss]
  );

  if (!current) return null;

  const conflictLabel = getConflictTypeLabel(current.conflictType);

  return (
    <div
      className="conflict-dialog-overlay"
      onClick={handleDismiss}
      style={{
        position: 'fixed',
        inset: 0,
        background: 'rgba(0, 0, 0, 0.5)',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        zIndex: 9999,
      }}
    >
      <div
        className="conflict-dialog"
        onClick={(e) => e.stopPropagation()}
        style={{
          background: 'var(--color-surface)',
          borderRadius: 8,
          padding: 16,
          maxWidth: 360,
          width: '90%',
          boxShadow: '0 10px 40px rgba(0,0,0,0.3)',
          border: '1px solid var(--color-border)',
        }}
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 8 }}>
          <span style={{ fontSize: 18 }}>⚠</span>
          <h3 style={{ fontSize: 14, fontWeight: 600, margin: 0 }}>
            Konflikt synchronizacji
          </h3>
        </div>

        <p style={{ fontSize: 12, color: 'var(--color-text-secondary)', marginBottom: 8 }}>
          {conflictLabel}
        </p>

        <div
          className="conflict-file"
          style={{
            background: 'var(--color-background)',
            padding: 8,
            borderRadius: 4,
            fontSize: 11,
            fontFamily: 'var(--font-mono, monospace)',
            marginBottom: 12,
            wordBreak: 'break-all',
          }}
        >
          {current.path}
        </div>

        <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
          <button
            type="button"
            className="btn btn-primary btn-sm"
            onClick={() => void handleResolve('keep_local')}
          >
            Zachowaj lokalną wersję
          </button>
          <button
            type="button"
            className="btn btn-primary btn-sm"
            onClick={() => void handleResolve('keep_remote')}
          >
            Zachowaj serwerową wersję
          </button>
          <button
            type="button"
            className="btn btn-outline btn-sm"
            onClick={() => void handleResolve('keep_both')}
          >
            Zachowaj obie (zmień nazwę lokalnej)
          </button>
          <button
            type="button"
            className="btn btn-outline btn-sm"
            onClick={() => void handleResolve('skip')}
          >
            Pomiń (zapytaj później)
          </button>
        </div>

        {queue.length - dismissedPaths.size > 1 && (
          <p
            style={{
              fontSize: 10,
              color: 'var(--color-text-secondary)',
              textAlign: 'center',
              marginTop: 8,
              marginBottom: 0,
            }}
          >
            Pozostało konfliktów: {queue.length - dismissedPaths.size - 1}
          </p>
        )}
      </div>
    </div>
  );
}

function getConflictTypeLabel(type: SyncConflictPayload['conflictType']): string {
  switch (type) {
    case 'BothModified':
      return 'Plik został zmodyfikowany zarówno lokalnie, jak i na serwerze.';
    case 'DeletedLocallyModifiedRemotely':
      return 'Plik został usunięty lokalnie, ale zmodyfikowany na serwerze.';
    case 'DeletedRemotelyModifiedLocally':
      return 'Plik został usunięty na serwerze, ale zmodyfikowany lokalnie.';
    default:
      return 'Wykryto konflikt podczas synchronizacji.';
  }
}
