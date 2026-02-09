import { useState, useCallback, useEffect } from 'react';
import { getConfig } from '@/lib/tauri';
import LoginPage from '@/pages/LoginPage';
import StatusPage from '@/pages/StatusPage';
import ActivityPage from '@/pages/ActivityPage';
import SettingsPage from '@/pages/SettingsPage';

type Page = 'status' | 'activity' | 'settings';

export default function App() {
  const [isLoggedIn, setIsLoggedIn] = useState(false);
  const [currentPage, setCurrentPage] = useState<Page>('status');
  const [loading, setLoading] = useState(true);

  // Check if already logged in on mount
  useEffect(() => {
    getConfig()
      .then((config) => {
        if (config.server_url && config.user_email) {
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
        <div
          className={`nav-item ${currentPage === 'status' ? 'active' : ''}`}
          onClick={() => setCurrentPage('status')}
        >
          Status
        </div>
        <div
          className={`nav-item ${currentPage === 'activity' ? 'active' : ''}`}
          onClick={() => setCurrentPage('activity')}
        >
          Aktywność
        </div>
        <div
          className={`nav-item ${currentPage === 'settings' ? 'active' : ''}`}
          onClick={() => setCurrentPage('settings')}
        >
          Ustawienia
        </div>
      </div>

      {currentPage === 'status' && <StatusPage />}
      {currentPage === 'activity' && <ActivityPage />}
      {currentPage === 'settings' && <SettingsPage onLogout={handleLogout} />}
    </div>
  );
}
