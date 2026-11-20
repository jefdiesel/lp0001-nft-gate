//! Minimal Logos Storage REST client for LP-0001 NFT media (requirement map row 30).
//!
//! Upload a file → CID, fetch by CID, and verify the fetched bytes against the media field of an
//! NFT's inline metadata. The REST shape is `POST /api/storage/v1/data` and
//! `GET /api/storage/v1/data/{cid}`. No chain access, proof or wallet is involved; the only tests
//! use a local fake HTTP server on an ephemeral loopback port, never a real network.
//!
//! The CID here is the test convention `b` + base32(0x12 0x20 ‖ sha256(bytes)) — a multibase /
//! multihash-shaped content id. Logos Storage's exact CID encoding must be checked against a real
//! endpoint before using this outside tests; `verify_media` only compares like with like.

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::Path;
#[cfg(test)]
use std::io::Write;
#[cfg(test)]
use std::net::TcpListener;
#[cfg(test)]
use std::thread::JoinHandle;
#[cfg(test)]
use std::time::Duration;

const BASE32: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
/// sha2-256 multihash prefix (code 0x12, length 0x20), included in the test CID.
const SHA256_PREFIX: [u8; 2] = [0x12, 0x20];

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum StorageError {
    #[error("I/O error: {0}")]
    Io(String),
    #[error("storage returned HTTP {status}: {body}")]
    Http { status: u16, body: String },
    #[error("bad storage response: {0}")]
    Response(String),
    #[error("metadata has no media field")]
    NoMedia,
    #[error("media hash mismatch: metadata says {expected}, bytes give {actual}")]
    MediaMismatch { expected: String, actual: String },
}

impl From<std::io::Error> for StorageError {
    fn from(error: std::io::Error) -> Self {
        StorageError::Io(error.to_string())
    }
}

/// RFC 4648 base32, lowercase, no padding (multibase `b`).
fn base32(data: &[u8]) -> String {
    let mut out = String::new();
    let mut buffer: u32 = 0;
    let mut bits = 0u32;
    for &byte in data {
        buffer = (buffer << 8) | u32::from(byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(BASE32[((buffer >> bits) & 31) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(BASE32[((buffer << (5 - bits)) & 31) as usize] as char);
    }
    out
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

/// The CID of these bytes under the test convention.
pub fn cid_from_bytes(bytes: &[u8]) -> String {
    let digest = sha256(bytes);
    let mut input = Vec::with_capacity(34);
    input.extend_from_slice(&SHA256_PREFIX);
    input.extend_from_slice(&digest);
    format!("b{}", base32(&input))
}

/// A small client for one Logos Storage endpoint.
pub struct StorageClient {
    base_url: String,
}

impl StorageClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
        }
    }

    fn endpoint(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    /// Upload bytes and return the CID from the storage response.
    pub fn upload_bytes(&self, bytes: &[u8]) -> Result<String, StorageError> {
        let response = call(
            ureq::post(&self.endpoint("/api/storage/v1/data"))
                .set("Content-Type", "application/octet-stream")
                .send(bytes),
        )?;
        let body = read_body(response)?;
        let text = String::from_utf8_lossy(&body).trim().to_string();
        if let Ok(value) = serde_json::from_str::<Value>(&text) {
            if let Some(cid) = value.get("cid").and_then(Value::as_str) {
                return Ok(cid.trim().to_string());
            }
            return Err(StorageError::Response(
                "upload response has no 'cid' field".into(),
            ));
        }
        if text.starts_with('b') && text.len() > 8 {
            Ok(text) // a server may answer with the bare CID
        } else {
            Err(StorageError::Response(format!(
                "unreadable upload response: {text}"
            )))
        }
    }

    /// Upload a file on disk.
    pub fn upload_file(&self, path: &Path) -> Result<String, StorageError> {
        let bytes = fs::read(path)?;
        self.upload_bytes(&bytes)
    }

    /// Fetch the bytes of a CID.
    pub fn fetch(&self, cid: &str) -> Result<Vec<u8>, StorageError> {
        let cid = media_cid(cid);
        let response = call(ureq::get(&self.endpoint(&format!("/api/storage/v1/data/{cid}"))).call())?;
        read_body(response)
    }

    /// Verify fetched bytes against an NFT's inline metadata media field.
    ///
    /// Accepts a `media`, `image` or `uri` string (a bare CID, or a `logos-storage:`/`ipfs:` URI),
    /// or a `sha256` / `mediaSha256` hex field. Any other shape is `NoMedia` — never silently pass.
    pub fn verify_media(
        &self,
        cid: &str,
        bytes: &[u8],
        metadata_json: &str,
    ) -> Result<(), StorageError> {
        let metadata: Value = serde_json::from_str(metadata_json)
            .map_err(|error| StorageError::Response(format!("metadata JSON: {error}")))?;

        if let Some(expected) = metadata
            .get("sha256")
            .or_else(|| metadata.get("mediaSha256"))
            .and_then(Value::as_str)
        {
            let actual = hex::encode(sha256(bytes));
            if expected.trim().eq_ignore_ascii_case(&actual) {
                return Ok(());
            }
            return Err(StorageError::MediaMismatch {
                expected: expected.to_string(),
                actual,
            });
        }

        let field = metadata
            .get("media")
            .or_else(|| metadata.get("image"))
            .or_else(|| metadata.get("uri"))
            .and_then(Value::as_str)
            .ok_or(StorageError::NoMedia)?;
        let expected = media_cid(field);
        let actual = cid_from_bytes(bytes);
        if expected == media_cid(cid) && expected == actual {
            return Ok(());
        }
        Err(StorageError::MediaMismatch { expected, actual })
    }
}

/// Strip a URI scheme/path down to the CID a metadata field points at.
fn media_cid(field: &str) -> String {
    let text = field.trim();
    let text = text
        .strip_prefix("logos-storage:")
        .or_else(|| text.strip_prefix("logos-storage://"))
        .or_else(|| text.strip_prefix("ipfs://"))
        .or_else(|| text.strip_prefix("ipfs:"))
        .unwrap_or(text);
    text.rsplit('/').next().unwrap_or(text).to_string()
}

/// ureq returns non-2xx as `Error::Status`, so translate it to our typed HTTP error before the
/// generic transport error branch hides the status code.
fn call(result: Result<ureq::Response, ureq::Error>) -> Result<ureq::Response, StorageError> {
    match result {
        Ok(response) => Ok(response),
        Err(ureq::Error::Status(status, response)) => {
            let mut body = Vec::new();
            let _ = response.into_reader().read_to_end(&mut body);
            Err(StorageError::Http {
                status,
                body: String::from_utf8_lossy(&body).into_owned(),
            })
        }
        Err(error) => Err(StorageError::Response(error.to_string())),
    }
}

fn read_body(response: ureq::Response) -> Result<Vec<u8>, StorageError> {
    let status = response.status();
    let mut bytes = Vec::new();
    response.into_reader().read_to_end(&mut bytes)?;
    if status >= 400 {
        return Err(StorageError::Http {
            status,
            body: String::from_utf8_lossy(&bytes).to_string(),
        });
    }
    Ok(bytes)
}

// ── Local fake HTTP server for tests (no real network) ───────────────────────────────

#[cfg(test)]
struct FakeStorage {
    url: String,
    // Kept so the server thread lives for the test; detached on drop (it blocks in accept()).
    _thread: JoinHandle<()>,
}

#[cfg(test)]
impl FakeStorage {
    fn start() -> Self {
        use std::collections::HashMap;
        use std::sync::{Arc, Mutex};

        let listener = TcpListener::bind("127.0.0.1:0").expect("loopback listen");
        let url = format!("http://{}", listener.local_addr().unwrap());
        let store: Arc<Mutex<HashMap<String, Vec<u8>>>> = Arc::new(Mutex::new(HashMap::new()));
        let server_store = store.clone();
        let thread = std::thread::spawn(move || {
            for stream in listener.incoming().take(8) {
                let Ok(mut stream) = stream else { continue };
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .expect("read timeout");
                let (header_end, header, data) = match read_request(&mut stream) {
                    Ok(parts) => parts,
                    Err(_) => continue,
                };
                let line = header.lines().next().unwrap_or_default();
                let mut parts = line.split_whitespace();
                let method = parts.next().unwrap_or_default();
                let path = parts.next().unwrap_or_default();
                let (status, body, content_type) = if method == "POST"
                    && path == "/api/storage/v1/data"
                {
                    let bytes = body_bytes(&header, &data, header_end);
                    let cid = cid_from_bytes(&bytes);
                    server_store.lock().unwrap().insert(cid.clone(), bytes);
                    (200, format!("{{\"cid\":\"{cid}\"}}").into_bytes(), "application/json")
                } else if method == "GET" && path.starts_with("/api/storage/v1/data/") {
                    let cid = path.trim_start_matches("/api/storage/v1/data/");
                    match server_store.lock().unwrap().get(cid).cloned() {
                        Some(bytes) => (200, bytes, "application/octet-stream"),
                        None => (404, b"not found".to_vec(), "text/plain"),
                    }
                } else {
                    (404, b"bad route".to_vec(), "text/plain")
                };
                let head = format!(
                    "HTTP/1.1 {} \r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    status,
                    content_type,
                    body.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&body);
                let _ = stream.flush();
            }
        });
        Self { url, _thread: thread }
    }
}

#[cfg(test)]
fn read_request(stream: &mut std::net::TcpStream) -> std::io::Result<(usize, String, Vec<u8>)> {
    let mut data = Vec::new();
    let mut chunk = [0u8; 4096];
    let header_end = loop {
        let count = stream.read(&mut chunk)?;
        if count == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "no request",
            ));
        }
        data.extend_from_slice(&chunk[..count]);
        if let Some(index) = data.windows(4).position(|window| window == b"\r\n\r\n") {
            break index + 4;
        }
    };
    let header = String::from_utf8_lossy(&data[..header_end]).to_string();
    if header.to_ascii_lowercase().contains("transfer-encoding: chunked") {
        while !data[header_end..]
            .windows(5)
            .any(|window| window == b"0\r\n\r\n")
        {
            let count = stream.read(&mut chunk)?;
            if count == 0 {
                break;
            }
            data.extend_from_slice(&chunk[..count]);
        }
    } else {
        let length = header
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                if name.trim().eq_ignore_ascii_case("content-length") {
                    value.trim().parse::<usize>().ok()
                } else {
                    None
                }
            })
            .unwrap_or(0);
        while data.len() < header_end + length {
            let count = stream.read(&mut chunk)?;
            if count == 0 {
                break;
            }
            data.extend_from_slice(&chunk[..count]);
        }
    }
    Ok((header_end, header, data))
}

/// Decode a request body sent with Content-Length or chunked Transfer-Encoding.
#[cfg(test)]
fn body_bytes(header: &str, data: &[u8], header_end: usize) -> Vec<u8> {
    let rest = &data[header_end..];
    if header.to_ascii_lowercase().contains("transfer-encoding: chunked") {
        let mut out = Vec::new();
        let mut at = 0;
        while at + 2 <= rest.len() {
            let end = rest[at..]
                .windows(2)
                .position(|w| w == b"\r\n")
                .map(|i| at + i)
                .unwrap_or(rest.len());
            let size_text = String::from_utf8_lossy(&rest[at..end]);
            let size = usize::from_str_radix(size_text.trim().split(';').next().unwrap_or("0"), 16)
                .unwrap_or(0);
            at = end + 2;
            if size == 0 {
                break;
            }
            if at + size > rest.len() {
                break;
            }
            out.extend_from_slice(&rest[at..at + size]);
            at += size + 2; // trailing CRLF
        }
        out
    } else {
        rest.to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IMAGE: &[u8] = b"a small test image, not a real NFT";

    #[test]
    fn cid_is_stable() {
        assert_eq!(cid_from_bytes(IMAGE), cid_from_bytes(IMAGE));
        assert_ne!(cid_from_bytes(IMAGE), cid_from_bytes(b"other"));
        assert!(cid_from_bytes(IMAGE).starts_with('b'));
    }

    #[test]
    fn upload_then_fetch_roundtrip() {
        let server = FakeStorage::start();
        let client = StorageClient::new(&server.url);
        let cid = client.upload_bytes(IMAGE).unwrap();
        assert_eq!(cid, cid_from_bytes(IMAGE));
        assert_eq!(client.fetch(&cid).unwrap(), IMAGE);
    }

    #[test]
    fn verify_media_accepts_a_matching_cid() {
        let server = FakeStorage::start();
        let client = StorageClient::new(&server.url);
        let cid = client.upload_bytes(IMAGE).unwrap();
        let metadata = format!("{{\"standard\":\"Expanded\",\"media\":\"logos-storage:{cid}\"}}");
        let bytes = client.fetch(&cid).unwrap();
        client.verify_media(&cid, &bytes, &metadata).unwrap();
    }

    #[test]
    fn verify_media_rejects_other_bytes() {
        let server = FakeStorage::start();
        let client = StorageClient::new(&server.url);
        let cid = client.upload_bytes(IMAGE).unwrap();
        let metadata = format!("{{\"image\":\"ipfs://{cid}\"}}");
        let error = client
            .verify_media(&cid, b"tampered bytes", &metadata)
            .unwrap_err();
        assert!(matches!(error, StorageError::MediaMismatch { .. }));
    }

    #[test]
    fn verify_media_checks_a_sha256_field() {
        let server = FakeStorage::start();
        let client = StorageClient::new(&server.url);
        let cid = client.upload_bytes(IMAGE).unwrap();
        let hash = hex::encode(sha256(IMAGE));
        let metadata = format!("{{\"mediaSha256\":\"{hash}\"}}");
        client.verify_media(&cid, IMAGE, &metadata).unwrap();
        let bad = format!("{{\"sha256\":\"{}\"}}", "00".repeat(32));
        assert!(matches!(
            client.verify_media(&cid, IMAGE, &bad),
            Err(StorageError::MediaMismatch { .. })
        ));
    }

    #[test]
    fn missing_media_never_passes() {
        let server = FakeStorage::start();
        let client = StorageClient::new(&server.url);
        let cid = client.upload_bytes(IMAGE).unwrap();
        assert_eq!(
            client.verify_media(&cid, IMAGE, "{\"standard\":\"Simple\"}"),
            Err(StorageError::NoMedia)
        );
    }

    #[test]
    fn unknown_cid_is_an_http_error() {
        let server = FakeStorage::start();
        let client = StorageClient::new(&server.url);
        assert!(matches!(
            client.fetch("bunknowncid123456"),
            Err(StorageError::Http { status: 404, .. })
        ));
    }
}
