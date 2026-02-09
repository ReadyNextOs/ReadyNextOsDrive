use keyring::Entry;
use serde::{Deserialize, Serialize};

const SERVICE_NAME: &str = "readynextos-drive";

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
pub fn store_token(email: &str, token: &AuthToken) -> Result<(), String> {
    let entry = Entry::new(SERVICE_NAME, email).map_err(|e| e.to_string())?;
    let json = serde_json::to_string(token).map_err(|e| e.to_string())?;
    entry.set_password(&json).map_err(|e| e.to_string())
}

/// Retrieve the auth token from the OS keychain.
pub fn get_token(email: &str) -> Result<Option<AuthToken>, String> {
    let entry = Entry::new(SERVICE_NAME, email).map_err(|e| e.to_string())?;
    match entry.get_password() {
        Ok(json) => {
            let token: AuthToken = serde_json::from_str(&json).map_err(|e| e.to_string())?;
            if token.is_expired() {
                // Remove expired token
                let _ = entry.delete_credential();
                Ok(None)
            } else {
                Ok(Some(token))
            }
        }
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

/// Remove the auth token from the OS keychain.
pub fn remove_token(email: &str) -> Result<(), String> {
    let entry = Entry::new(SERVICE_NAME, email).map_err(|e| e.to_string())?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

/// Login response from the server
#[derive(Debug, Deserialize)]
pub struct LoginResponse {
    pub token: String,
    pub user: LoginUser,
}

#[derive(Debug, Deserialize)]
pub struct LoginUser {
    pub id: String,
    pub email: String,
    pub name: String,
    pub tenant_id: String,
}

/// Login with email and password, returns Sanctum API token.
pub async fn login(
    server_url: &str,
    email: &str,
    password: &str,
) -> Result<LoginResponse, String> {
    let client = reqwest::Client::new();
    let url = format!("{}/api/v1/auth/login", server_url.trim_end_matches('/'));

    let response = client
        .post(&url)
        .json(&serde_json::json!({
            "email": email,
            "password": password,
            "device_name": "ReadyNextOs Drive",
        }))
        .send()
        .await
        .map_err(|e| format!("Connection error: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Login failed ({}): {}", status, body));
    }

    response
        .json::<LoginResponse>()
        .await
        .map_err(|e| format!("Invalid response: {}", e))
}
