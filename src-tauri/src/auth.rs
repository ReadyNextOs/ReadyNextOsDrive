use crate::error::{AppError, AppResult};
use keyring::Entry;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const SERVICE_NAME: &str = "veloryn-cloudfile";

/// Authentication token stored securely in OS keychain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthToken {
    pub token: String,
    pub token_type: TokenType,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TokenType {
    Sanctum,
    Jwt,
}

impl AuthToken {
    /// Check if the token is expired
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            chrono::Utc::now() >= expires_at
        } else {
            false
        }
    }
}

/// Store the auth token in the OS keychain.
pub fn store_token(email: &str, token: &AuthToken) -> AppResult<()> {
    log::info!(
        "store_token: storing for email={}, type={:?}, expires_at={:?}",
        email,
        token.token_type,
        token.expires_at
    );
    let entry = Entry::new(SERVICE_NAME, email).map_err(|e| {
        log::error!("store_token: failed to create keychain entry: {}", e);
        AppError::auth(format!("Nie udało się utworzyć wpisu keychain: {}", e))
    })?;
    let json = serde_json::to_string(token)
        .map_err(|e| AppError::auth(format!("Nie udało się zserializować tokenu: {}", e)))?;
    entry.set_password(&json).map_err(|e| {
        log::error!("store_token: failed to save to keychain: {}", e);
        AppError::auth(format!("Nie udało się zapisać tokenu: {}", e))
    })?;
    log::info!("store_token: token saved successfully for {}", email);
    Ok(())
}

/// Retrieve the auth token from the OS keychain.
pub fn get_token(email: &str) -> AppResult<Option<AuthToken>> {
    log::info!("get_token: looking up token for email={}", email);
    let entry = Entry::new(SERVICE_NAME, email).map_err(|e| {
        log::error!("get_token: failed to create keychain entry: {}", e);
        AppError::auth(format!("Nie udało się utworzyć wpisu keychain: {}", e))
    })?;
    match entry.get_password() {
        Ok(json) => {
            log::info!("get_token: found token in keychain (len={})", json.len());
            let token: AuthToken = serde_json::from_str(&json)
                .map_err(|e| AppError::auth(format!("Nie udało się odczytać tokenu: {}", e)))?;
            if token.is_expired() {
                log::warn!(
                    "get_token: token expired at {:?}, removing",
                    token.expires_at
                );
                let _ = entry.delete_credential();
                Ok(None)
            } else {
                log::info!("get_token: token valid, type={:?}", token.token_type);
                Ok(Some(token))
            }
        }
        Err(keyring::Error::NoEntry) => {
            log::warn!("get_token: no token found in keychain for {}", email);
            Ok(None)
        }
        Err(e) => {
            log::error!("get_token: keychain error: {}", e);
            Err(AppError::auth(format!(
                "Nie udało się pobrać tokenu: {}",
                e
            )))
        }
    }
}

/// Remove the auth token from the OS keychain.
pub fn remove_token(email: &str) -> AppResult<()> {
    let entry = Entry::new(SERVICE_NAME, email)
        .map_err(|e| AppError::auth(format!("Nie udało się utworzyć wpisu keychain: {}", e)))?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(AppError::auth(format!(
            "Nie udało się usunąć tokenu: {}",
            e
        ))),
    }
}

/// Login response from the server
#[derive(Debug, Deserialize)]
pub struct LoginResponse {
    pub token: String,
    pub user: LoginUser,
}

#[derive(Debug, Deserialize)]
pub struct DesktopTokenExchangeResponse {
    pub token: String,
    pub user: LoginUser,
    pub config: DesktopTokenConfig,
}

#[derive(Debug, Deserialize)]
pub struct DesktopTokenConfig {
    pub server_url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginUser {
    pub id: String,
    pub email: String,
    pub name: String,
    pub tenant_id: String,
}

/// Login with email and password, returns Sanctum API token.
pub async fn login(server_url: &str, email: &str, password: &str) -> AppResult<LoginResponse> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| AppError::network(format!("Nie udało się zbudować klienta HTTP: {}", e)))?;
    let url = format!("{}/api/v1/auth/login", server_url.trim_end_matches('/'));

    let response = client
        .post(&url)
        .json(&serde_json::json!({
            "email": email,
            "password": password,
            "device_name": "Veloryn CloudFile",
        }))
        .send()
        .await
        .map_err(|e| AppError::network(format!("Błąd połączenia: {}", e)))?;

    if !response.status().is_success() {
        return Err(match response.status().as_u16() {
            401 => AppError::auth("Nieprawidłowy e-mail lub hasło"),
            403 => AppError::auth("Logowanie zostało odrzucone przez serwer"),
            408 | 504 => AppError::network("Serwer nie odpowiedział na czas"),
            status => AppError::auth(format!("Logowanie nie powiodło się (HTTP {})", status)),
        });
    }

    response
        .json::<LoginResponse>()
        .await
        .map_err(|e| AppError::network(format!("Nieprawidłowa odpowiedź serwera: {}", e)))
}

/// Exchange a short-lived desktop bootstrap token for the normal API token.
pub async fn exchange_desktop_token(
    bootstrap_token: &str,
) -> AppResult<DesktopTokenExchangeResponse> {
    let trimmed_token = bootstrap_token.trim();
    if trimmed_token.is_empty() {
        return Err(AppError::auth("Token jest wymagany"));
    }

    let claims = decode_desktop_token_claims(trimmed_token)?;
    let server_url = claims
        .server_url
        .ok_or_else(|| AppError::auth("Token desktopowy nie zawiera server_url"))?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| AppError::network(format!("Nie udało się zbudować klienta HTTP: {}", e)))?;
    let url = format!(
        "{}/api/v1/desktop-auth/exchange",
        server_url.trim_end_matches('/')
    );

    let response = client
        .post(&url)
        .json(&serde_json::json!({
            "token": trimmed_token,
            "device_name": "Veloryn CloudFile",
        }))
        .send()
        .await
        .map_err(|e| AppError::network(format!("Błąd połączenia: {}", e)))?;

    if !response.status().is_success() {
        return Err(match response.status().as_u16() {
            401 => AppError::auth("Token desktopowy jest nieprawidłowy"),
            409 => AppError::auth("Token desktopowy został już wykorzystany"),
            410 => AppError::auth("Token desktopowy wygasł"),
            408 | 504 => AppError::network("Serwer nie odpowiedział na czas"),
            status => AppError::auth(format!("Wymiana tokenu nie powiodła się (HTTP {})", status)),
        });
    }

    response
        .json::<DesktopTokenExchangeResponse>()
        .await
        .map_err(|e| AppError::network(format!("Nieprawidłowa odpowiedź serwera: {}", e)))
}

#[derive(Debug, Deserialize)]
struct DesktopTokenClaims {
    server_url: Option<String>,
}

fn decode_desktop_token_claims(token: &str) -> AppResult<DesktopTokenClaims> {
    let payload = token
        .split('.')
        .nth(1)
        .ok_or_else(|| AppError::auth("Token desktopowy ma nieprawidłowy format"))?;
    let decoded =
        base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, payload)
            .map_err(|e| {
                AppError::auth(format!("Nieprawidłowy payload tokenu desktopowego: {}", e))
            })?;

    serde_json::from_slice::<DesktopTokenClaims>(&decoded)
        .map_err(|e| AppError::auth(format!("Nieprawidłowe claims tokenu desktopowego: {}", e)))
}
