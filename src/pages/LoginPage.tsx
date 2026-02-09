import { useState, useCallback, FormEvent } from 'react';
import { login } from '@/lib/tauri';

interface LoginPageProps {
  onLoginSuccess: () => void;
}

export default function LoginPage({ onLoginSuccess }: LoginPageProps) {
  const [serverUrl, setServerUrl] = useState('');
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');

  const handleSubmit = useCallback(async (e: FormEvent) => {
    e.preventDefault();
    setError('');
    setLoading(true);

    try {
      await login(serverUrl, email, password);
      onLoginSuccess();
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, [serverUrl, email, password, onLoginSuccess]);

  return (
    <div className="container">
      <div className="logo">
        <h1>ReadyNextOs Drive</h1>
        <p>Zaloguj się, aby zsynchronizować pliki</p>
      </div>

      <form onSubmit={handleSubmit}>
        <div className="card">
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

          {error && <p className="error">{error}</p>}

          <button
            type="submit"
            className="btn btn-primary"
            disabled={loading || !serverUrl || !email || !password}
            style={{ marginTop: 8 }}
          >
            {loading ? 'Logowanie...' : 'Zaloguj się'}
          </button>
        </div>
      </form>
    </div>
  );
}
