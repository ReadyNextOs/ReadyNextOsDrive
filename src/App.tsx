import { useState, useCallback, useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import { getConfig } from '@/lib/tauri';
import LoginPage from '@/pages/LoginPage';
import StatusPage from '@/pages/StatusPage';
import ActivityPage from '@/pages/ActivityPage';
import SettingsPage from '@/pages/SettingsPage';
import UpdatePage from '@/pages/UpdatePage';

type Page = 'status' | 'activity' | 'settings' | 'update';

export default function App() {
  const [isLoggedIn, setIsLoggedIn] = useState(false);
  const [currentPage, setCurrentPage] = useState<Page>('status');
  const [loading, setLoading] = useState(true);

  // Check if already logged in on mount
  useEffect(() => {
    getConfig()
      .then((config) => {
        if (config.server_url && config.user_login) {
          setIsLoggedIn(true);
        }
      })
      .catch(() => {})
      .finally(() => setLoading(false));
  }, []);

  const handleLoginSuccess = useCallback(() => {
    setIsLoggedIn(true);
    setCurrentPage('status');
  }, []);

  const handleLogout = useCallback(() => {
    setIsLoggedIn(false);
    setCurrentPage('status');
  }, []);

  // Listen for navigation events from tray menu
  useEffect(() => {
    const unlisten = listen<string>('navigate', (event) => {
      const page = event.payload as Page;
      if (['status', 'activity', 'settings', 'update'].includes(page)) {
        setCurrentPage(page);
      }
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  // Listen for background update-available event
  const [updateAvailable, setUpdateAvailable] = useState(false);
  useEffect(() => {
    const unlisten = listen<string>('update-available', () => {
      setUpdateAvailable(true);
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  if (loading) {
    return (
      <div className="container" style={{ textAlign: 'center', paddingTop: 40 }}>
        <p style={{ color: 'var(--color-text-secondary)' }}>Ładowanie...</p>
      </div>
    );
  }

  if (!isLoggedIn) {
    return <LoginPage onLoginSuccess={handleLoginSuccess} />;
  }

  return (
    <div>
      <div className="nav">
        <button
          type="button"
          className={`nav-item ${currentPage === 'status' ? 'active' : ''}`}
          onClick={() => setCurrentPage('status')}
        >
          Status
        </button>
        <button
          type="button"
          className={`nav-item ${currentPage === 'activity' ? 'active' : ''}`}
          onClick={() => setCurrentPage('activity')}
        >
          Aktywność
        </button>
        <button
          type="button"
          className={`nav-item ${currentPage === 'settings' ? 'active' : ''}`}
          onClick={() => setCurrentPage('settings')}
        >
          Ustawienia
        </button>
        <button
          type="button"
          className={`nav-item ${currentPage === 'update' ? 'active' : ''}`}
          onClick={() => { setCurrentPage('update'); setUpdateAvailable(false); }}
          style={{ position: 'relative' }}
        >
          Aktualizacja
          {updateAvailable && <span className="update-dot" />}
        </button>
      </div>

      <div className="page-scroll">
        {currentPage === 'status' && <StatusPage />}
        {currentPage === 'activity' && <ActivityPage />}
        {currentPage === 'settings' && <SettingsPage onLogout={handleLogout} />}
        {currentPage === 'update' && <UpdatePage />}
      </div>
    </div>
  );
}
