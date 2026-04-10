use crate::error::{AppError, AppResult};
use reqwest::Client;
use std::path::Path;
use std::time::Duration;

/// WebDAV file transfer operations.
/// All methods use Bearer token authentication.
pub struct WebDavTransfer {
    client: Client,
    base_url: String,
    token: String,
    /// Upload speed limit in bytes/sec (0 = unlimited)
    upload_limit_bps: u64,
    /// Download speed limit in bytes/sec (0 = unlimited)
    download_limit_bps: u64,
}

#[derive(Debug, Clone)]
pub struct UploadResult {
    pub remote_path: String,
    pub bytes_sent: u64,
}

#[derive(Debug, Clone)]
pub struct DownloadResult {
    pub local_path: String,
    pub bytes_received: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RemoteEntry {
    pub path: String,
    pub name: String,
    pub is_directory: bool,
    pub size: Option<u64>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

impl WebDavTransfer {
    /// Create a new transfer client.
    /// base_url: e.g. "https://dev.veloryn.pl/backend/dav/personal"
    pub fn new(base_url: &str, token: &str) -> Self {
        Self::new_with_limits(base_url, token, 0, 0)
    }

    /// Create a new transfer client with bandwidth limits.
    /// Limits are in KB/s (0 = unlimited).
    pub fn new_with_limits(
        base_url: &str,
        token: &str,
        upload_limit_kbps: u64,
        download_limit_kbps: u64,
    ) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(300))
            .build()
            .expect("Failed to build reqwest client");
        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            token: token.to_string(),
            upload_limit_bps: upload_limit_kbps * 1024,
            download_limit_bps: download_limit_kbps * 1024,
        }
    }

    /// Apply rate limiting by sleeping proportionally to data transferred.
    async fn rate_limit(&self, bytes: u64, limit_bps: u64) {
        if limit_bps == 0 || bytes == 0 {
            return;
        }
        let expected_duration_ms = (bytes as f64 / limit_bps as f64 * 1000.0) as u64;
        if expected_duration_ms > 0 {
            tokio::time::sleep(Duration::from_millis(expected_duration_ms)).await;
        }
    }

    fn build_url(&self, remote_path: &str) -> String {
        let encoded = encode_path(remote_path);
        format!("{}/{}", self.base_url, encoded)
    }

    fn auth_header(&self) -> String {
        format!("Bearer {}", self.token)
    }

    /// Upload a local file to WebDAV remote path.
    /// Uses HTTP PUT with file body.
    pub async fn upload_file(
        &self,
        local_path: &Path,
        remote_path: &str,
    ) -> AppResult<UploadResult> {
        let bytes = tokio::fs::read(local_path)
            .await
            .map_err(|e| AppError::io(format!("Failed to read file {}: {}", local_path.display(), e)))?;
        let bytes_len = bytes.len() as u64;

        // Ensure parent directory (and all ancestors) exist on remote.
        // WebDAV MKCOL is not recursive, so we create each segment in turn.
        if let Some((parent, _)) = remote_path.trim_start_matches('/').rsplit_once('/') {
            if !parent.is_empty() {
                self.mkcol_recursive(parent).await.ok();
            }
        }

        let url = self.build_url(remote_path);
        let result = retry_request(3, |attempt| {
            let url = url.clone();
            let auth = self.auth_header();
            let bytes = bytes.clone();
            let client = self.client.clone();
            async move {
                log::debug!("upload_file attempt {}: PUT {}", attempt, url);
                let resp = client
                    .put(&url)
                    .header("Authorization", &auth)
                    .header("Content-Type", "application/octet-stream")
                    .body(bytes)
                    .send()
                    .await
                    .map_err(|e| AppError::network(format!("Network error uploading {}: {}", url, e)))?;

                let status = resp.status();
                if status.is_success() || status.as_u16() == 201 || status.as_u16() == 204 {
                    Ok(())
                } else {
                    Err(RetryableError {
                        error: AppError::network(format!("HTTP {}: {}", status, url)),
                        retryable: status.as_u16() >= 500,
                    })
                }
            }
        })
        .await;

        // Apply bandwidth throttling after successful transfer
        if result.is_ok() {
            self.rate_limit(bytes_len, self.upload_limit_bps).await;
        }

        result.map(|_| UploadResult {
            remote_path: remote_path.to_string(),
            bytes_sent: bytes_len,
        })
    }

    /// Download a file from WebDAV to local path.
    /// Uses HTTP GET, streams response to file.
    pub async fn download_file(
        &self,
        remote_path: &str,
        local_path: &Path,
    ) -> AppResult<DownloadResult> {
        // Ensure local parent directory exists
        if let Some(parent) = local_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| AppError::io(format!("Failed to create directory {}: {}", parent.display(), e)))?;
        }

        let url = self.build_url(remote_path);
        let bytes = retry_request(3, |attempt| {
            let url = url.clone();
            let auth = self.auth_header();
            let client = self.client.clone();
            async move {
                log::debug!("download_file attempt {}: GET {}", attempt, url);
                let resp = client
                    .get(&url)
                    .header("Authorization", &auth)
                    .send()
                    .await
                    .map_err(|e| AppError::network(format!("Network error downloading {}: {}", url, e)))?;

                let status = resp.status();
                if !status.is_success() {
                    return Err(RetryableError {
                        error: AppError::network(format!("HTTP {}: {}", status, url)),
                        retryable: status.as_u16() >= 500,
                    });
                }

                let bytes = resp
                    .bytes()
                    .await
                    .map_err(|e| AppError::network(format!("Failed to read response body: {}", e)))?;
                Ok(bytes)
            }
        })
        .await?;

        let bytes_len = bytes.len() as u64;
        tokio::fs::write(local_path, &bytes)
            .await
            .map_err(|e| AppError::io(format!("Failed to write file {}: {}", local_path.display(), e)))?;

        // Apply bandwidth throttling after successful transfer
        self.rate_limit(bytes_len, self.download_limit_bps).await;

        Ok(DownloadResult {
            local_path: local_path.to_string_lossy().to_string(),
            bytes_received: bytes_len,
        })
    }

    /// Delete a remote file/directory via WebDAV DELETE.
    pub async fn delete_remote(&self, remote_path: &str) -> AppResult<()> {
        let url = self.build_url(remote_path);
        retry_request(3, |attempt| {
            let url = url.clone();
            let auth = self.auth_header();
            let client = self.client.clone();
            async move {
                log::debug!("delete_remote attempt {}: DELETE {}", attempt, url);
                let resp = client
                    .delete(&url)
                    .header("Authorization", &auth)
                    .send()
                    .await
                    .map_err(|e| AppError::network(format!("Network error deleting {}: {}", url, e)))?;

                let status = resp.status();
                match status.as_u16() {
                    200 | 204 | 404 => Ok(()),
                    _ => Err(RetryableError {
                        error: AppError::network(format!("HTTP {}: {}", status, url)),
                        retryable: status.as_u16() >= 500,
                    }),
                }
            }
        })
        .await
    }

    /// List directory contents via WebDAV PROPFIND (depth 1).
    /// Returns file/directory metadata for sync comparison.
    pub async fn propfind(&self, remote_path: &str) -> AppResult<Vec<RemoteEntry>> {
        const PROPFIND_BODY: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:getcontentlength/>
    <D:getetag/>
    <D:getlastmodified/>
    <D:resourcetype/>
  </D:prop>
</D:propfind>"#;

        let url = self.build_url(remote_path);
        let body = retry_request(3, |attempt| {
            let url = url.clone();
            let auth = self.auth_header();
            let client = self.client.clone();
            async move {
                log::debug!("propfind attempt {}: PROPFIND {}", attempt, url);
                let resp = client
                    .request(reqwest::Method::from_bytes(b"PROPFIND").unwrap(), &url)
                    .header("Authorization", &auth)
                    .header("Depth", "1")
                    .header("Content-Type", "application/xml; charset=utf-8")
                    .body(PROPFIND_BODY)
                    .send()
                    .await
                    .map_err(|e| AppError::network(format!("Network error in PROPFIND {}: {}", url, e)))?;

                let status = resp.status();
                if status.as_u16() != 207 && !status.is_success() {
                    return Err(RetryableError {
                        error: AppError::network(format!("HTTP {}: {}", status, url)),
                        retryable: status.as_u16() >= 500,
                    });
                }

                let text = resp
                    .text()
                    .await
                    .map_err(|e| AppError::network(format!("Failed to read PROPFIND response: {}", e)))?;
                Ok(text)
            }
        })
        .await?;

        parse_propfind_response(&body, remote_path)
    }

    /// Create a remote directory via WebDAV MKCOL.
    pub async fn mkcol(&self, remote_path: &str) -> AppResult<()> {
        let url = self.build_url(remote_path);
        retry_request(3, |attempt| {
            let url = url.clone();
            let auth = self.auth_header();
            let client = self.client.clone();
            async move {
                log::debug!("mkcol attempt {}: MKCOL {}", attempt, url);
                let resp = client
                    .request(reqwest::Method::from_bytes(b"MKCOL").unwrap(), &url)
                    .header("Authorization", &auth)
                    .send()
                    .await
                    .map_err(|e| AppError::network(format!("Network error in MKCOL {}: {}", url, e)))?;

                let status = resp.status();
                match status.as_u16() {
                    // 201 = created, 405 = already exists (Method Not Allowed on existing collection)
                    201 | 405 => Ok(()),
                    _ => Err(RetryableError {
                        error: AppError::network(format!("HTTP {}: {}", status, url)),
                        retryable: status.as_u16() >= 500,
                    }),
                }
            }
        })
        .await
    }

    /// Recursively create a directory tree on the remote.
    /// For `folder/sub/deep`, creates `folder`, then `folder/sub`, then `folder/sub/deep`.
    /// Ignores per-level failures (409 Conflict happens if parent missing) since the
    /// sequential walk creates the missing parents on subsequent iterations.
    pub async fn mkcol_recursive(&self, remote_path: &str) -> AppResult<()> {
        let trimmed = remote_path.trim_start_matches('/').trim_end_matches('/');
        if trimmed.is_empty() {
            return Ok(());
        }

        let segments: Vec<&str> = trimmed.split('/').filter(|s| !s.is_empty()).collect();
        let mut current = String::new();
        for seg in segments {
            if !current.is_empty() {
                current.push('/');
            }
            current.push_str(seg);
            // Best-effort: ignore errors on intermediate levels.
            let _ = self.mkcol(&current).await;
        }
        Ok(())
    }
}

// ==================== Helpers ====================

/// Encode a WebDAV path: percent-encode each segment, preserve slashes.
fn encode_path(path: &str) -> String {
    path.trim_start_matches('/')
        .split('/')
        .map(|segment| urlencoding::encode(segment).into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// Error wrapper that carries whether a retry is worthwhile.
struct RetryableError {
    error: AppError,
    retryable: bool,
}

impl From<AppError> for RetryableError {
    fn from(error: AppError) -> Self {
        Self {
            error,
            retryable: false,
        }
    }
}

/// Run an async operation with exponential backoff retry.
/// Retries up to `max_attempts` times on retryable errors (5xx or network).
/// Delays: 1s → 2s → 4s.
async fn retry_request<F, Fut, T>(max_attempts: u32, mut f: F) -> AppResult<T>
where
    F: FnMut(u32) -> Fut,
    Fut: std::future::Future<Output = Result<T, RetryableError>>,
{
    let mut last_error = AppError::network("No attempts made");
    for attempt in 1..=max_attempts {
        match f(attempt).await {
            Ok(value) => return Ok(value),
            Err(RetryableError { error, retryable }) => {
                last_error = error;
                if !retryable || attempt == max_attempts {
                    break;
                }
                let delay = Duration::from_secs(2u64.pow(attempt - 1)); // 1s, 2s, 4s
                log::warn!(
                    "Request failed (attempt {}/{}), retrying in {:?}: {}",
                    attempt,
                    max_attempts,
                    delay,
                    last_error
                );
                tokio::time::sleep(delay).await;
            }
        }
    }
    Err(last_error)
}

/// Parse a WebDAV PROPFIND XML response into a list of RemoteEntry.
/// Uses simple string parsing to avoid adding an xml crate dependency.
fn parse_propfind_response(xml: &str, request_path: &str) -> AppResult<Vec<RemoteEntry>> {
    let mut entries = Vec::new();

    // Split into <D:response> blocks
    let mut remaining = xml;
    while let Some(start) = find_tag_start(remaining, "response") {
        remaining = &remaining[start..];
        let end = find_tag_end(remaining, "response").unwrap_or(remaining.len());
        let block = &remaining[..end];
        remaining = &remaining[end..];

        if let Some(entry) = parse_response_block(block) {
            entries.push(entry);
        }
    }

    // Filter out the root entry (the requested directory itself)
    let normalized_request = normalize_webdav_path(request_path);
    entries.retain(|e| {
        let normalized = normalize_webdav_path(&e.path);
        normalized != normalized_request
    });

    Ok(entries)
}

/// Find the byte offset of the start of a tag (opening `<` including namespace prefix variants).
fn find_tag_start(haystack: &str, tag_local: &str) -> Option<usize> {
    // Match <D:response>, <response>, <d:response>, etc.
    for prefix in &["<D:", "<d:", "<"] {
        let needle = format!("{}{}", prefix, tag_local);
        if let Some(pos) = haystack.find(needle.as_str()) {
            return Some(pos);
        }
    }
    None
}

/// Find the end of a closing tag block (past the `</D:response>` or variant).
fn find_tag_end(haystack: &str, tag_local: &str) -> Option<usize> {
    for prefix in &["</D:", "</d:", "</"] {
        let needle = format!("{}{}>", prefix, tag_local);
        if let Some(pos) = haystack.find(needle.as_str()) {
            return Some(pos + needle.len());
        }
    }
    None
}

/// Extract the text content of the first matching element in a block.
fn extract_element_text<'a>(block: &'a str, tag_local: &str) -> Option<&'a str> {
    for prefix in &["D:", "d:", ""] {
        let open = format!("<{}{}>", prefix, tag_local);
        let close = format!("</{}{}>", prefix, tag_local);
        if let Some(start) = block.find(open.as_str()) {
            let after_open = start + open.len();
            if let Some(end) = block[after_open..].find(close.as_str()) {
                return Some(&block[after_open..after_open + end]);
            }
        }
    }
    None
}

/// Parse a single <response> block into a RemoteEntry.
fn parse_response_block(block: &str) -> Option<RemoteEntry> {
    // Extract href
    let href = extract_element_text(block, "href")?;
    let href = href.trim().to_string();

    // Determine if directory: <resourcetype> contains <collection>
    let is_directory = block.contains("collection");

    // Extract size
    let size: Option<u64> = extract_element_text(block, "getcontentlength")
        .and_then(|s| s.trim().parse().ok());

    // Extract etag (strip surrounding quotes if present)
    let etag = extract_element_text(block, "getetag").map(|s| {
        let s = s.trim();
        let s = s.trim_matches('"');
        s.to_string()
    });

    // Extract last modified
    let last_modified = extract_element_text(block, "getlastmodified")
        .map(|s| s.trim().to_string());

    // Derive name from href path
    let decoded_href = urlencoding::decode(&href)
        .map(|cow| cow.into_owned())
        .unwrap_or_else(|_| href.clone());
    let name = decoded_href
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(&decoded_href)
        .to_string();

    if name.is_empty() {
        return None;
    }

    Some(RemoteEntry {
        path: decoded_href,
        name,
        is_directory,
        size,
        etag,
        last_modified,
    })
}

/// Normalize a WebDAV path for comparison (remove trailing slash, decode).
fn normalize_webdav_path(path: &str) -> String {
    let decoded = urlencoding::decode(path)
        .map(|cow| cow.into_owned())
        .unwrap_or_else(|_| path.to_string());
    decoded.trim_end_matches('/').to_lowercase()
}
