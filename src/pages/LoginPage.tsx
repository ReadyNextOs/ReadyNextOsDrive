import { useState, useCallback, FormEvent } from 'react';
import { login, loginWithToken } from '@/lib/tauri';

interface LoginPageProps {
  onLoginSuccess: () => void;
}

export default function LoginPage({ onLoginSuccess }: LoginPageProps) {
  const [mode, setMode] = useState<'token' | 'advanced'>('token');
  const [serverUrl, setServerUrl] = useState('');
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [token, setToken] = useState('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');

  const handlePasswordSubmit = useCallback(async (e: FormEvent) => {
    e.preventDefault();
    setError('');
    setLoading(true);

    try {
      await login(serverUrl, email, password);
      onLoginSuccess();
    } catch (err) {
      setError(formatLoginError(err));
    } finally {
      setLoading(false);
    }
  }, [serverUrl, email, password, onLoginSuccess]);

  const handleTokenSubmit = useCallback(async (e: FormEvent) => {
    e.preventDefault();
    setError('');
    setLoading(true);

    try {
      await loginWithToken(token);
      onLoginSuccess();
    } catch (err) {
      setError(formatLoginError(err));
    } finally {
      setLoading(false);
    }
  }, [token, onLoginSuccess]);

  return (
    <div className="container">
      <div className="logo">
        <h1>ReadyNextOs Drive</h1>
        <p>Połącz aplikację, aby zsynchronizować pliki</p>
      </div>

      <div className="card" style={{ marginBottom: 16 }}>
        <div style={{ display: 'flex', gap: 8 }}>
          <button
            type="button"
            className={`btn ${mode === 'token' ? 'btn-primary' : ''}`}
            onClick={() => {
              setMode('token');
              setError('');
            }}
            style={{ flex: 1 }}
          >
            Token
          </button>
          <button
            type="button"
            className={`btn ${mode === 'advanced' ? 'btn-primary' : ''}`}
            onClick={() => {
              setMode('advanced');
              setError('');
            }}
            style={{ flex: 1 }}
          >
            Zaawansowane
          </button>
        </div>
      </div>

      <form onSubmit={mode === 'token' ? handleTokenSubmit : handlePasswordSubmit}>
        <div className="card">
          {mode === 'token' ? (
            <>
              <div className="input-group">
                <label htmlFor="token">Token logowania</label>
                <input
                  id="token"
                  type="text"
                  className="input"
                  placeholder="Wklej token z panelu ReadyNextOs"
                  value={token}
                  onChange={(e) => setToken(e.target.value)}
                  required
                />
              </div>
              <p style={{ color: 'var(--color-text-secondary)', marginTop: 0 }}>
                Token powinien być krótkowieczny i jednorazowy. Aplikacja pobierze z niego adres
                serwera i wymieni go na właściwy token API.
              </p>
            </>
          ) : (
            <>
              <div className="input-group">
                <label htmlFor="server">Adres serwera</label>
                <input
                  id="server"
                  type="url"
                  className="input"
                  placeholder="https://docs.firma.pl"
                  value={serverUrl}
                  onChange={(e) => setServerUrl(e.target.value)}
                  required
                />
              </div>

              <div className="input-group">
                <label htmlFor="email">E-mail</label>
                <input
                  id="email"
                  type="email"
                  className="input"
                  placeholder="jan@firma.pl"
                  value={email}
                  onChange={(e) => setEmail(e.target.value)}
                  required
                />
              </div>

              <div className="input-group">
                <label htmlFor="password">Hasło</label>
                <input
                  id="password"
                  type="password"
                  className="input"
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  required
                />
              </div>
            </>
          )}

          {error && <p className="error">{error}</p>}

          <button
            type="submit"
            className="btn btn-primary"
            disabled={
              loading ||
              (mode === 'token'
                ? !token.trim()
                : !serverUrl.trim() || !email.trim() || !password)
            }
            style={{ marginTop: 8 }}
          >
            {loading ? 'Logowanie...' : mode === 'token' ? 'Połącz tokenem' : 'Zaloguj się'}
          </button>
        </div>
      </form>
    </div>
  );
}

function formatLoginError(err: unknown): string {
  const raw = String(err);
  // Tauri invoke errors are prefixed with various internal strings — strip them
  const cleaned = raw
    .replace(/^Error:\s*/i, '')
    .replace(/^invoke\s+error:\s*/i, '')
    .replace(/^Tauri\s+error:\s*/i, '')
    .trim();
  return cleaned || 'Wystąpił nieznany błąd. Spróbuj ponownie.';
}
