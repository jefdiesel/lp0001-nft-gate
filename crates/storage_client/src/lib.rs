//! Minimal Logos Storage REST client for LP-0001 NFT media (requirement map row 30).
//!
//! Upload a file → CID, fetch by CID, and verify fetched bytes against an NFT's inline metadata.
//! REST shape, checked against a real Logos Storage node (reviewer, 2026-11-20):
//! - `POST /api/storage/v1/data` answers `200 text/plain` with a bare base58btc CID such as
//!   `zDvZRwzm78buYYJGskHsMFmAQ9PbhZRRg8DgLUMhiGoUAqKewsiC`;
//! - `GET /api/storage/v1/data/{cid}/network/stream` returns the bytes from the network (and works
//!   for local content too): [`StorageClient::fetch`];
//! - `GET /api/storage/v1/data/{cid}` returns them from the node's local store only:
//!   [`StorageClient::fetch_local`].
//!
//! A Logos Storage CID is a manifest CID over a block tree, so it cannot be recomputed from the
//! bytes. The real integrity check is a hash in the metadata (`mediaSha256`, else `sha256`); a
//! CID match only says the bytes came from the CID the metadata names. [`Verified`] says which.
//! No chain access, proof or wallet is involved; tests use a local fake HTTP server only.

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

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum StorageError {
    #[error("I/O error: {0}")]
    Io(String),
    #[error("storage returned HTTP {status}: {body}")]
    Http { status: u16, body: String },
    #[error("bad storage response: {0}")]
    Response(String),
    #[error("metadata has no media hash or CID field")]
    NoMedia,
    #[error("media hash mismatch: metadata says {expected}, bytes give {actual}")]
    MediaMismatch { expected: String, actual: String },
    #[error("CID mismatch: metadata names {expected}, bytes were fetched from {actual}")]
    CidMismatch { expected: String, actual: String },
}

impl From<std::io::Error> for StorageError {
    fn from(error: std::io::Error) -> Self {
        StorageError::Io(error.to_string())
    }
}

/// What [`StorageClient::verify_media`] actually checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verified {
    /// The bytes hash to the metadata's `mediaSha256`/`sha256`: the real integrity check.
    Hash,
    /// No hash in the metadata: only the CID string was matched (the bytes came from the CID the
    /// metadata names). The bytes themselves were not checked.
    CidOnly,
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

/// Read a CID out of an upload response: a bare CID in text/plain (the real node), or a JSON
/// object with a `cid` field (kept as a fallback).
pub fn parse_upload_response(text: &str) -> Result<String, StorageError> {
    let text = text.trim();
    if let Ok(value) = serde_json::from_str::<Value>(text) {
        return match value.get("cid").and_then(Value::as_str) {
            Some(cid) if is_cid_text(cid.trim()) => Ok(cid.trim().to_string()),
            _ => Err(StorageError::Response("upload response has no usable 'cid' field".into())),
        };
    }
    if is_cid_text(text) {
        Ok(text.to_string())
    } else {
        Err(StorageError::Response(format!("unreadable upload response: {text}")))
    }
}

/// A bare multibase CID: 8 to 256 characters, letters and digits only (base58btc `z…`, base32
/// `b…` and the like). Rejects HTML, JSON fragments and error text.
fn is_cid_text(text: &str) -> bool {
    (8..=256).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// A small client for one Logos Storage endpoint.
pub struct StorageClient {
    base_url: String,
}

impl StorageClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self { base_url: base_url.into().trim_end_matches('/').to_string() }
    }

    fn endpoint(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    /// Upload bytes and return the CID the node gives back.
    pub fn upload_bytes(&self, bytes: &[u8]) -> Result<String, StorageError> {
        let response = call(
            ureq::post(&self.endpoint("/api/storage/v1/data"))
                .set("Content-Type", "application/octet-stream")
                .send(bytes),
        )?;
        let body = read_body(response)?;
        parse_upload_response(&String::from_utf8_lossy(&body))
    }

    /// Upload a file on disk.
    pub fn upload_file(&self, path: &Path) -> Result<String, StorageError> {
        let bytes = fs::read(path)?;
        self.upload_bytes(&bytes)
    }

    /// Fetch a CID's bytes from the network (`/network/stream`; also serves local content).
    pub fn fetch(&self, cid: &str) -> Result<Vec<u8>, StorageError> {
        let cid = media_cid(cid);
        let url = self.endpoint(&format!("/api/storage/v1/data/{cid}/network/stream"));
        read_body(call(ureq::get(&url).call())?)
    }

    /// Fetch a CID's bytes from the node's local store only (404 when it isn't stored there).
    pub fn fetch_local(&self, cid: &str) -> Result<Vec<u8>, StorageError> {
        let cid = media_cid(cid);
        read_body(call(ureq::get(&self.endpoint(&format!("/api/storage/v1/data/{cid}"))).call())?)
    }

    /// Verify bytes fetched from `cid` against an NFT's inline metadata.
    ///
    /// A hash field wins (`mediaSha256`, then `sha256`): the bytes must hash to it ([`Verified::Hash`]).
    /// Without one, the metadata's `media`/`image`/`uri` CID (bare, or a `logos-storage:`/`ipfs:`
    /// URI) must equal the CID the bytes were fetched from ([`Verified::CidOnly`]: the bytes are not
    /// checked). Anything else is `NoMedia`: never a silent pass. Uploaders should always write
    /// `mediaSha256`.
    pub fn verify_media(&self, cid: &str, bytes: &[u8], metadata_json: &str) -> Result<Verified, StorageError> {
        let metadata: Value = serde_json::from_str(metadata_json)
            .map_err(|error| StorageError::Response(format!("metadata JSON: {error}")))?;

        if let Some(expected) = metadata
            .get("mediaSha256")
            .or_else(|| metadata.get("sha256"))
            .and_then(Value::as_str)
        {
            let actual = hex::encode(sha256(bytes));
            return if expected.trim().eq_ignore_ascii_case(&actual) {
                Ok(Verified::Hash)
            } else {
                Err(StorageError::MediaMismatch { expected: expected.to_string(), actual })
            };
        }

        let field = metadata
            .get("media")
            .or_else(|| metadata.get("image"))
            .or_else(|| metadata.get("uri"))
            .and_then(Value::as_str)
            .ok_or(StorageError::NoMedia)?;
        let expected = media_cid(field);
        let actual = media_cid(cid);
        if !expected.is_empty() && expected == actual {
            Ok(Verified::CidOnly)
        } else {
            Err(StorageError::CidMismatch { expected, actual })
        }
    }
}

/// Strip a URI scheme/path down to the CID a metadata field points at.
fn media_cid(field: &str) -> String {
    let text = field.trim();
    let text = text
        .strip_prefix("logos-storage://")
        .or_else(|| text.strip_prefix("logos-storage:"))
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
            Err(StorageError::Http { status, body: String::from_utf8_lossy(&body).into_owned() })
        }
        Err(error) => Err(StorageError::Response(error.to_string())),
    }
}

fn read_body(response: ureq::Response) -> Result<Vec<u8>, StorageError> {
    let status = response.status();
    let mut bytes = Vec::new();
    response.into_reader().read_to_end(&mut bytes)?;
    if status >= 400 {
        return Err(StorageError::Http { status, body: String::from_utf8_lossy(&bytes).to_string() });
    }
    Ok(bytes)
}

// ── Local fake HTTP server for tests (no real network) ───────────────────────────────

/// The fake server's CID: `z` + base58btc(sha256(bytes)). Shaped like a real node's answer but
/// NOT how Logos Storage derives CIDs (those are manifest CIDs); test code only.
#[cfg(test)]
fn fake_server_cid(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 58] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    let mut digits: Vec<u8> = vec![0];
    for &byte in sha256(bytes).iter() {
        let mut carry = u32::from(byte);
        for digit in digits.iter_mut() {
            carry += u32::from(*digit) << 8;
            *digit = (carry % 58) as u8;
            carry /= 58;
        }
        while carry > 0 {
            digits.push((carry % 58) as u8);
            carry /= 58;
        }
    }
    let mut out = String::from("z");
    out.extend(digits.iter().rev().map(|&d| ALPHABET[d as usize] as char));
    out
}

/// Answers exactly like a real node: POST → `200 text/plain` bare `z…` CID; GET `/data/{cid}`
/// (local) and `/data/{cid}/network/stream`; anything else, including `/network`, is 404.
#[cfg(test)]
struct FakeStorage {
    url: String,
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
        let thread = std::thread::spawn(move || {
            for stream in listener.incoming().take(16) {
                let Ok(mut stream) = stream else { continue };
                stream.set_read_timeout(Some(Duration::from_secs(2))).expect("read timeout");
                let Ok((header_end, header, data)) = read_request(&mut stream) else { continue };
                let line = header.lines().next().unwrap_or_default();
                let mut parts = line.split_whitespace();
                let method = parts.next().unwrap_or_default();
                let path = parts.next().unwrap_or_default();
                let get = |cid: &str| match store.lock().unwrap().get(cid).cloned() {
                    Some(bytes) => (200, bytes, "application/octet-stream"),
                    None => (404, b"not found".to_vec(), "text/plain"),
                };
                let (status, body, content_type) = if method == "POST" && path == "/api/storage/v1/data" {
                    let bytes = body_bytes(&header, &data, header_end);
                    let cid = fake_server_cid(&bytes);
                    store.lock().unwrap().insert(cid.clone(), bytes);
                    (200, cid.into_bytes(), "text/plain")
                } else if method == "GET" {
                    match path.strip_prefix("/api/storage/v1/data/") {
                        Some(rest) => match rest.strip_suffix("/network/stream") {
                            Some(cid) if !cid.contains('/') => get(cid),
                            _ if !rest.contains('/') => get(rest),
                            _ => (404, b"bad route".to_vec(), "text/plain"),
                        },
                        None => (404, b"bad route".to_vec(), "text/plain"),
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
    /// A CID a real Logos Storage node returned (reviewer, M2, 2026-11-20 15:46Z).
    const REAL_CID: &str = "zDvZRwzm78buYYJGskHsMFmAQ9PbhZRRg8DgLUMhiGoUAqKewsiC";

    fn upload() -> (FakeStorage, StorageClient, String) {
        let server = FakeStorage::start();
        let client = StorageClient::new(&server.url);
        let cid = client.upload_bytes(IMAGE).unwrap();
        (server, client, cid)
    }

    #[test]
    fn a_real_nodes_upload_answer_parses() {
        assert_eq!(parse_upload_response(&format!("{REAL_CID}\n")).unwrap(), REAL_CID);
        assert_eq!(parse_upload_response(&format!("{{\"cid\":\"{REAL_CID}\"}}")).unwrap(), REAL_CID);
        assert!(parse_upload_response("<html>error</html>").is_err());
        assert!(parse_upload_response("{\"error\":\"x\"}").is_err());
        assert!(parse_upload_response("").is_err());
    }

    #[test]
    fn upload_returns_a_bare_z_cid_then_fetch_roundtrips() {
        let (_server, client, cid) = upload();
        assert!(cid.starts_with('z'), "{cid}");
        assert_eq!(cid, fake_server_cid(IMAGE));
        assert_eq!(client.fetch(&cid).unwrap(), IMAGE); // /network/stream
        assert_eq!(client.fetch_local(&cid).unwrap(), IMAGE); // /data/{cid}
    }

    #[test]
    fn verify_prefers_the_hash() {
        let (_server, client, cid) = upload();
        let bytes = client.fetch(&cid).unwrap();
        let hash = hex::encode(sha256(IMAGE));
        let metadata = format!("{{\"media\":\"logos-storage:{cid}\",\"mediaSha256\":\"{hash}\"}}");
        assert_eq!(client.verify_media(&cid, &bytes, &metadata), Ok(Verified::Hash));
        assert!(matches!(
            client.verify_media(&cid, b"tampered bytes", &metadata),
            Err(StorageError::MediaMismatch { .. })
        ));
        let bad = format!("{{\"sha256\":\"{}\"}}", "00".repeat(32));
        assert!(matches!(client.verify_media(&cid, IMAGE, &bad), Err(StorageError::MediaMismatch { .. })));
    }

    #[test]
    fn without_a_hash_only_the_cid_is_matched_and_it_says_so() {
        let (_server, client, cid) = upload();
        let metadata = format!("{{\"image\":\"ipfs://{cid}\"}}");
        assert_eq!(client.verify_media(&cid, IMAGE, &metadata), Ok(Verified::CidOnly));
        let real = format!("{{\"media\":\"logos-storage://{REAL_CID}\"}}");
        assert_eq!(client.verify_media(REAL_CID, b"any bytes", &real), Ok(Verified::CidOnly));
        assert!(matches!(client.verify_media(&cid, IMAGE, &real), Err(StorageError::CidMismatch { .. })));
    }

    #[test]
    fn missing_media_never_passes() {
        let (_server, client, cid) = upload();
        assert_eq!(client.verify_media(&cid, IMAGE, "{\"standard\":\"Simple\"}"), Err(StorageError::NoMedia));
    }

    #[test]
    fn unknown_cid_and_bare_network_path_are_404() {
        let server = FakeStorage::start();
        let client = StorageClient::new(&server.url);
        assert!(matches!(client.fetch(REAL_CID), Err(StorageError::Http { status: 404, .. })));
        assert!(matches!(client.fetch_local(REAL_CID), Err(StorageError::Http { status: 404, .. })));
        let url = format!("{}/api/storage/v1/data/{REAL_CID}/network", server.url);
        assert!(matches!(call(ureq::get(&url).call()), Err(StorageError::Http { status: 404, .. })));
    }
}
