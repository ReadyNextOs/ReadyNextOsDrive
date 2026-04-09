import { useState, useEffect, useCallback } from 'react';
import { listen } from '@tauri-apps/api/event';

interface SyncProgress {
  phase: string;
  file_count: number;
  message: string;
  source: string;
}

function pluralizeFiles(count: number): string {
  if (count === 1) return 'plik';
  const lastTwo = count % 100;
  const lastOne = count % 10;
  if (lastTwo >= 12 && lastTwo <= 14) return 'plików';
  if (lastOne >= 2 && lastOne <= 4) return 'pliki';
  return 'plików';
}

export default function SyncToast() {
  const [toast, setToast] = useState<{
    message: string;
    type: 'info' | 'success' | 'error';
  } | null>(null);

  const showToast = useCallback(
    (message: string, type: 'info' | 'success' | 'error') => {
      setToast({ message, type });
    },
    []
  );

  useEffect(() => {
    const unlisten = listen<SyncProgress>('sync-progress', (event) => {
      const { phase, file_count, source } = event.payload;

      if (phase === 'started') {
        if (source === 'manual' || source === 'watcher') {
          showToast('Synchronizacja...', 'info');
        }
      } else if (phase === 'completed') {
        if (file_count > 0) {
          showToast(
            `Zsynchronizowano ${file_count} ${pluralizeFiles(file_count)}`,
            'success'
          );
        } else {
          setToast((prev) => (prev?.type === 'info' ? null : prev));
        }
      } else if (phase === 'error') {
        showToast('Błąd synchronizacji', 'error');
      }
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, [showToast]);

  useEffect(() => {
    if (!toast) return;
    const duration =
      toast.type === 'error' ? 6000 : toast.type === 'info' ? 30000 : 4000;
    const timer = setTimeout(() => setToast(null), duration);
    return () => clearTimeout(timer);
  }, [toast]);

  if (!toast) return null;

  return (
    <div
      className={`sync-toast sync-toast-${toast.type}`}
      onClick={() => setToast(null)}
    >
      <span className={`sync-toast-icon sync-toast-icon-${toast.type}`} />
      <span>{toast.message}</span>
    </div>
  );
}
