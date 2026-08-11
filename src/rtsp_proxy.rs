//! Loopback-only RTSP authentication proxy.
//!
//! The P2P tunnel intentionally remains a transparent TCP relay. This proxy
//! is the credential boundary for local processes such as FFmpeg: camera
//! credentials stay in camrelay memory and are never placed in a gateway
//! process argument or sent to another service.

use base64::{engine::general_purpose::STANDARD, Engine};
use md5::{Digest, Md5};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tokio::{
    io::{copy_bidirectional, AsyncRead, AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task::AbortHandle,
};

use crate::config::Camera;

#[derive(Clone)]
pub struct RtspProxyManager {
    entries: Arc<Mutex<HashMap<String, ProxyEntry>>>,
}

struct ProxyEntry {
    port: u16,
    abort: AbortHandle,
}

impl RtspProxyManager {
    pub fn new() -> Self {
        Self {
            entries: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn ensure(&self, camera: Camera) -> Result<u16, String> {
        if let Some(entry) = self.entries.lock().unwrap().get(&camera.id) {
            return Ok(entry.port);
        }

        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .map_err(|error| format!("Could not bind RTSP proxy: {error}"))?;
        let port = listener
            .local_addr()
            .map_err(|error| format!("Could not read RTSP proxy address: {error}"))?
            .port();
        let camera_for_task = camera.clone();
        let task = tokio::spawn(async move {
            loop {
                let (client, _) = match listener.accept().await {
                    Ok(connection) => connection,
                    Err(error) => {
                        eprintln!("RTSP proxy accept failed: {error}");
                        break;
                    }
                };
                let camera = camera_for_task.clone();
                tokio::spawn(async move {
                    if let Err(error) = proxy_connection(client, camera).await {
                        eprintln!("RTSP proxy connection closed: {error}");
                    }
                });
            }
        });

        let entry = ProxyEntry {
            port,
            abort: task.abort_handle(),
        };
        let mut entries = self.entries.lock().unwrap();
        if let Some(existing) = entries.get(&camera.id) {
            task.abort();
            return Ok(existing.port);
        }
        entries.insert(camera.id, entry);
        Ok(port)
    }

    pub fn stop(&self, camera_id: &str) {
        if let Some(entry) = self.entries.lock().unwrap().remove(camera_id) {
            entry.abort.abort();
        }
    }
}

async fn proxy_connection(mut client: TcpStream, camera: Camera) -> Result<(), String> {
    let mut upstream = TcpStream::connect(("127.0.0.1", camera.local_port))
        .await
        .map_err(|error| format!("Could not connect to local RTSP tunnel: {error}"))?;
    let mut auth = None;

    loop {
        let request = read_rtsp_frame(&mut client)
            .await
            .map_err(|error| format!("Could not read RTSP request: {error}"))?;
        if request.is_empty() {
            return Ok(());
        }

        let method = rtsp_method(&request).unwrap_or_default();
        let mut outbound = auth
            .as_mut()
            .and_then(|state| make_authorized_request(&request, state, &camera))
            .unwrap_or_else(|| request.clone());
        upstream
            .write_all(&outbound)
            .await
            .map_err(|error| format!("Could not write RTSP request: {error}"))?;

        let mut response = read_rtsp_frame(&mut upstream)
            .await
            .map_err(|error| format!("Could not read RTSP response: {error}"))?;
        if response_status(&response) == Some(401) {
            if let Some(challenge) = header_value(&response, "WWW-Authenticate")
                .and_then(|value| parse_challenge(&value))
            {
                let mut state = AuthState {
                    challenge,
                    nonce_count: 0,
                    cnonce: format!("{:016x}", rand::random::<u64>()),
                };
                outbound = make_authorized_request(&request, &mut state, &camera)
                    .ok_or_else(|| "Could not construct RTSP authorization".to_string())?;
                upstream
                    .write_all(&outbound)
                    .await
                    .map_err(|error| format!("Could not retry RTSP request: {error}"))?;
                response = read_rtsp_frame(&mut upstream).await.map_err(|error| {
                    format!("Could not read authenticated RTSP response: {error}")
                })?;
                auth = Some(state);
            }
        }

        client
            .write_all(&response)
            .await
            .map_err(|error| format!("Could not write RTSP response: {error}"))?;

        if method.eq_ignore_ascii_case("PLAY")
            && response_status(&response).is_some_and(|status| (200..300).contains(&status))
        {
            copy_bidirectional(&mut client, &mut upstream)
                .await
                .map_err(|error| format!("RTSP media stream closed: {error}"))?;
            return Ok(());
        }
    }
}

#[derive(Clone)]
struct AuthState {
    challenge: AuthChallenge,
    nonce_count: u32,
    cnonce: String,
}

#[derive(Clone)]
enum AuthChallenge {
    Basic,
    Digest {
        realm: String,
        nonce: String,
        qop: Option<String>,
        opaque: Option<String>,
        algorithm: String,
    },
}

fn make_authorized_request(
    request: &[u8],
    state: &mut AuthState,
    camera: &Camera,
) -> Option<Vec<u8>> {
    let method = rtsp_method(request)?;
    let uri = rtsp_uri(request)?;
    state.nonce_count = state.nonce_count.saturating_add(1);
    let authorization = match &state.challenge {
        AuthChallenge::Basic => format!(
            "Basic {}",
            STANDARD.encode(format!("{}:{}", camera.username, camera.password))
        ),
        AuthChallenge::Digest {
            realm,
            nonce,
            qop,
            opaque,
            algorithm,
        } => {
            if !algorithm.eq_ignore_ascii_case("MD5") && !algorithm.is_empty() {
                return None;
            }
            let nonce_count = format!("{:08x}", state.nonce_count);
            let ha1 = md5_hex(&format!(
                "{}:{}:{}",
                camera.username, realm, camera.password
            ));
            let ha2 = md5_hex(&format!("{}:{}", method, uri));
            let response = match qop.as_deref() {
                Some(qop) => md5_hex(&format!(
                    "{}:{}:{}:{}:{}:{}",
                    ha1, nonce, nonce_count, state.cnonce, qop, ha2
                )),
                None => md5_hex(&format!("{}:{}:{}", ha1, nonce, ha2)),
            };
            let mut value = format!(
                "Digest username=\"{}\", realm=\"{}\", nonce=\"{}\", uri=\"{}\", response=\"{}\"",
                camera.username, realm, nonce, uri, response
            );
            if let Some(qop) = qop {
                value.push_str(&format!(
                    ", qop={qop}, nc={nonce_count}, cnonce=\"{}\"",
                    state.cnonce
                ));
            }
            if let Some(opaque) = opaque {
                value.push_str(&format!(", opaque=\"{opaque}\""));
            }
            value
        }
    };
    Some(with_header(request, "Authorization", &authorization))
}

fn parse_challenge(value: &str) -> Option<AuthChallenge> {
    let (scheme, parameters) = value.split_once(' ')?;
    let values = parse_auth_parameters(parameters);
    match scheme.to_ascii_lowercase().as_str() {
        "basic" => Some(AuthChallenge::Basic),
        "digest" => Some(AuthChallenge::Digest {
            realm: values.get("realm")?.clone(),
            nonce: values.get("nonce")?.clone(),
            qop: values.get("qop").and_then(|qop| {
                qop.split(',')
                    .map(str::trim)
                    .find(|value| value.eq_ignore_ascii_case("auth"))
                    .map(str::to_string)
            }),
            opaque: values.get("opaque").cloned(),
            algorithm: values
                .get("algorithm")
                .cloned()
                .unwrap_or_else(|| "MD5".to_string()),
        }),
        _ => None,
    }
}

fn parse_auth_parameters(value: &str) -> HashMap<String, String> {
    value
        .split(',')
        .filter_map(|part| {
            let (key, value) = part.split_once('=')?;
            Some((
                key.trim().to_ascii_lowercase(),
                value.trim().trim_matches('"').to_string(),
            ))
        })
        .collect()
}

fn with_header(message: &[u8], name: &str, value: &str) -> Vec<u8> {
    let marker = b"\r\n\r\n";
    let Some(split) = message
        .windows(marker.len())
        .position(|window| window == marker)
    else {
        return message.to_vec();
    };
    let body = &message[split + marker.len()..];
    let head = String::from_utf8_lossy(&message[..split]);
    let mut output = head
        .lines()
        .filter(|line| !line.to_ascii_lowercase().starts_with("authorization:"))
        .collect::<Vec<_>>()
        .join("\r\n");
    output.push_str(&format!("\r\n{name}: {value}\r\n\r\n"));
    let mut result = output.into_bytes();
    result.extend_from_slice(body);
    result
}

async fn read_rtsp_frame<R>(reader: &mut R) -> std::io::Result<Vec<u8>>
where
    R: AsyncRead + Unpin,
{
    let mut first = [0u8; 1];
    if reader.read_exact(&mut first).await.is_err() {
        return Ok(Vec::new());
    }
    if first[0] == b'$' {
        let mut header = [0u8; 3];
        reader.read_exact(&mut header).await?;
        let length = u16::from_be_bytes([header[1], header[2]]) as usize;
        let mut frame = vec![b'$', header[0], header[1], header[2]];
        let mut payload = vec![0u8; length];
        reader.read_exact(&mut payload).await?;
        frame.extend(payload);
        return Ok(frame);
    }

    let mut message = vec![first[0]];
    loop {
        let mut byte = [0u8; 1];
        reader.read_exact(&mut byte).await?;
        message.push(byte[0]);
        if message.len() >= 4 && &message[message.len() - 4..] == b"\r\n\r\n" {
            break;
        }
    }
    let content_length = header_value(&message, "Content-Length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    if content_length > 0 {
        let mut body = vec![0u8; content_length];
        reader.read_exact(&mut body).await?;
        message.extend(body);
    }
    Ok(message)
}

fn header_value(message: &[u8], name: &str) -> Option<String> {
    String::from_utf8_lossy(message).lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.trim()
            .eq_ignore_ascii_case(name)
            .then(|| value.trim().to_string())
    })
}

fn rtsp_method(message: &[u8]) -> Option<String> {
    String::from_utf8_lossy(message)
        .lines()
        .next()?
        .split_whitespace()
        .next()
        .map(str::to_string)
}

fn rtsp_uri(message: &[u8]) -> Option<String> {
    String::from_utf8_lossy(message)
        .lines()
        .next()?
        .split_whitespace()
        .nth(1)
        .map(str::to_string)
}

fn response_status(message: &[u8]) -> Option<u16> {
    String::from_utf8_lossy(message)
        .lines()
        .next()?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

fn md5_hex(value: &str) -> String {
    let mut digest = Md5::new();
    digest.update(value.as_bytes());
    format!("{:x}", digest.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn camera() -> Camera {
        Camera {
            id: "cam-1".to_string(),
            name: "Test camera".to_string(),
            brand: "Dahua".to_string(),
            serial: "SERIAL".to_string(),
            username: "admin".to_string(),
            password: "secret".to_string(),
            port: 554,
            local_port: 8551,
            auto_start: false,
        }
    }

    #[test]
    fn basic_authorization_is_added_without_plaintext_password() {
        let request = b"DESCRIBE rtsp://127.0.0.1:8551/cam RTSP/1.0\r\nCSeq: 1\r\n\r\n";
        let mut state = AuthState {
            challenge: AuthChallenge::Basic,
            nonce_count: 0,
            cnonce: "test".to_string(),
        };
        let authorized = make_authorized_request(request, &mut state, &camera()).unwrap();
        let text = String::from_utf8(authorized).unwrap();
        assert!(text.contains("Authorization: Basic YWRtaW46c2VjcmV0"));
        assert!(!text.contains("secret"));
    }

    #[test]
    fn digest_challenge_is_parsed_and_authorization_is_scoped_to_request() {
        let challenge =
            parse_challenge("Digest realm=\"cam\", nonce=\"abc\", qop=\"auth\", opaque=\"opaque\"")
                .unwrap();
        let mut state = AuthState {
            challenge,
            nonce_count: 0,
            cnonce: "fixed-cnonce".to_string(),
        };
        let request = b"DESCRIBE rtsp://127.0.0.1:8551/cam RTSP/1.0\r\nCSeq: 1\r\nContent-Length: 4\r\n\r\nbody";
        let authorized = make_authorized_request(request, &mut state, &camera()).unwrap();
        let text = String::from_utf8_lossy(&authorized);
        assert!(text.contains("Authorization: Digest"));
        assert!(text.contains("nc=00000001"));
        assert!(text.ends_with("\r\n\r\nbody"));
        assert!(!text.contains("secret"));
    }
}
