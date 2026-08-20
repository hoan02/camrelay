use async_trait::async_trait;
use base64::Engine;
use sha1::Digest;
use std::{collections::HashMap, net::SocketAddrV4};
use tokio::{net::UdpSocket, time};
use xml::reader::{EventReader, XmlEvent};

use crate::ptcp::{PTCPBody, PTCPSession, Ptcp};

const DH_READ_TIMEOUT: time::Duration = time::Duration::from_secs(8);
const DEVICE_HANDSHAKE_TIMEOUT: time::Duration = time::Duration::from_secs(5);

/// Performs only the provider-level DH probe. This validates that the configured
/// endpoint accepts the supplied platform credentials; it does not connect to
/// a camera or prove RTSP compatibility.
pub async fn probe_provider(
    main_server: &str,
    app_username: &str,
    app_userkey: &str,
) -> Result<String, String> {
    if main_server.trim().is_empty()
        || app_username.trim().is_empty()
        || app_userkey.trim().is_empty()
    {
        return Err("Provider endpoint and platform credentials are required.".to_string());
    }

    let socket = UdpSocket::bind("0.0.0.0:0")
        .await
        .map_err(|e| format!("Could not bind probe socket: {e}"))?;
    socket
        .connect(main_server)
        .await
        .map_err(|e| format!("Could not reach provider endpoint: {e}"))?;

    let nonce = rand::random::<u32>();
    let created = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let password = format!("{}{}DHP2P:{}:{}", nonce, created, app_username, app_userkey);
    let mut hasher = sha1::Sha1::new();
    hasher.update(password);
    let digest = base64::engine::general_purpose::STANDARD.encode(hasher.finalize());
    let request = format!(
        "DHGET /probe/p2psrv HTTP/1.1\r\nCSeq: 1\r\nAuthorization: WSSE profile=\"UsernameToken\"\r\nX-WSSE: UsernameToken Username=\"{}\", PasswordDigest=\"{}\", Nonce=\"{}\", Created=\"{}\"\r\n\r\n",
        app_username, digest, nonce, created
    );

    socket
        .send(request.as_bytes())
        .await
        .map_err(|e| format!("Could not send provider probe: {e}"))?;

    let mut buffer = [0u8; 4096];
    let received = time::timeout(time::Duration::from_secs(8), socket.recv(&mut buffer))
        .await
        .map_err(|_| "Provider probe timed out after 8 seconds.".to_string())?
        .map_err(|e| format!("Could not read provider response: {e}"))?;
    let response = String::from_utf8_lossy(&buffer[..received]);
    let status = response
        .lines()
        .next()
        .unwrap_or("Unknown provider response");
    let code = status
        .split_whitespace()
        .nth(1)
        .and_then(|value| value.parse::<u16>().ok())
        .ok_or_else(|| "Provider returned an invalid response.".to_string())?;

    if code >= 300 {
        return Err(format!("Provider rejected the probe ({status})."));
    }
    Ok(format!("Provider probe accepted ({status})."))
}

fn ip_to_bytes(ip: &str) -> Result<Vec<u8>, String> {
    let addr: SocketAddrV4 = ip
        .parse()
        .map_err(|e| format!("Invalid peer address '{ip}': {e}"))?;
    let ip = addr.ip().octets();
    let port = addr.port();

    let mut bytes = Vec::new();
    bytes.extend_from_slice(&port.to_be_bytes());
    bytes.extend_from_slice(&ip);

    Ok(bytes.iter().map(|b| !b).collect())
}

fn required_body_value<'a>(
    body: &'a HashMap<String, String>,
    key: &str,
    step: &str,
) -> Result<&'a str, String> {
    body.get(key)
        .map(String::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("{step} response did not contain required field {key}"))
}

async fn recv_with_timeout(
    socket: &UdpSocket,
    buffer: &mut [u8],
    timeout: time::Duration,
    step: &str,
) -> Result<usize, String> {
    time::timeout(timeout, socket.recv(buffer))
        .await
        .map_err(|_| format!("Timed out waiting {} seconds for {step}", timeout.as_secs()))?
        .map_err(|e| format!("Could not read {step}: {e}"))
}

pub async fn p2p_handshake(
    socket: UdpSocket,
    serial: String,
    relay_mode: bool,
    main_server: &str,
    app_username: &str,
    app_userkey: &str,
) -> Result<(UdpSocket, PTCPSession), String> {
    let mut cseq = 0;

    socket
        .connect(main_server)
        .await
        .map_err(|e| format!("Could not connect to provider endpoint: {e}"))?;

    socket
        .dh_request("/probe/p2psrv", None, &mut cseq, app_username, app_userkey)
        .await?;
    socket.dh_read().await?;

    socket
        .dh_request(
            format!("/online/p2psrv/{}", serial).as_ref(),
            None,
            &mut cseq,
            app_username,
            app_userkey,
        )
        .await?;
    let p2psrv_body = socket.dh_read().await?.body.ok_or_else(|| {
        "Online provider response did not contain a body with the P2P server".to_string()
    })?;
    let p2psrv = required_body_value(&p2psrv_body, "body/US", "Online provider")?;

    socket
        .dh_request("/online/relay", None, &mut cseq, app_username, app_userkey)
        .await?;
    let relay_body = socket.dh_read().await?.body.ok_or_else(|| {
        "Relay discovery response did not contain a body with the relay address".to_string()
    })?;
    let relay = required_body_value(&relay_body, "body/Address", "Relay discovery")?;

    let socket2 = UdpSocket::bind("0.0.0.0:0")
        .await
        .map_err(|e| format!("Could not bind P2P control socket: {e}"))?;
    socket2
        .connect(p2psrv)
        .await
        .map_err(|e| format!("Could not connect to P2P server {p2psrv}: {e}"))?;

    socket2
        .dh_request(
            format!("/probe/device/{}", serial).as_ref(),
            None,
            &mut cseq,
            app_username,
            app_userkey,
        )
        .await?;
    socket2.dh_read().await?;

    /*
    Device info is intentionally not requested here until a provider/device
    contract is confirmed. Keeping the request out avoids changing the
    handshake sequence for implementations that do not support this endpoint.
    socket2
        .dh_request(
            format!("/info/device/{}", serial).as_ref(),
            None,
            &mut cseq,
        )
        .await;
    socket2.dh_read().await;
    */

    let cid: [u8; 8] = rand::random();

    socket
        .dh_request(
            format!("/device/{}/p2p-channel", serial).as_ref(),
            Some(format!(
                "<body><Identify>{}</Identify><IpEncrpt>true</IpEncrpt><LocalAddr>127.0.0.1:{}</LocalAddr><version>5.0.0</version></body>",
                cid.iter().map(|b| format!("{:x}", b)).collect::<Vec<_>>().join(" "),
                socket
                    .local_addr()
                    .map_err(|e| format!("Could not inspect local P2P socket: {e}"))?
                    .port(),
            ).as_ref()),
            &mut cseq,
            app_username,
            app_userkey,
        )
        .await?;

    socket2
        .connect(relay)
        .await
        .map_err(|e| format!("Could not connect to relay {relay}: {e}"))?;

    socket2
        .dh_request("/relay/agent", None, &mut cseq, app_username, app_userkey)
        .await?;
    let data = socket2
        .dh_read()
        .await?
        .body
        .ok_or_else(|| "Relay agent response did not contain a body".to_string())?;
    let token = required_body_value(&data, "body/Token", "Relay agent")?;
    let agent = required_body_value(&data, "body/Agent", "Relay agent")?;

    socket2
        .connect(agent)
        .await
        .map_err(|e| format!("Could not connect to relay agent {agent}: {e}"))?;

    socket2
        .dh_request(
            format!("/relay/start/{}", token).as_ref(),
            Some("<body><Client>:0</Client></body>"),
            &mut cseq,
            app_username,
            app_userkey,
        )
        .await?;
    socket2.dh_read().await?;

    let mut res = socket.dh_read_raw().await?;

    if res.code == 100 {
        res = socket.dh_read_raw().await?;
    }

    if res.code >= 400 {
        let hint = if res.code == 403 {
            " Device authentication is required and is not supported at this time."
        } else {
            ""
        };
        return Err(format!(
            "P2P channel creation failed: {}.{hint}",
            res.status
        ));
    }

    let data = res
        .body
        .ok_or_else(|| "P2P channel response did not contain a body".to_string())?;
    let device_laddr = required_body_value(&data, "body/LocalAddr", "P2P channel")?;
    let device = required_body_value(&data, "body/PubAddr", "P2P channel")?;

    // not necessary when relay_mode is true, but UDP is connectionless
    socket
        .connect(device)
        .await
        .map_err(|e| format!("Could not connect to device peer {device}: {e}"))?;

    socket2
        .connect(main_server)
        .await
        .map_err(|e| format!("Could not reconnect control socket to provider: {e}"))?;

    socket2
        .dh_request(
            format!("/device/{}/relay-channel", serial).as_ref(),
            Some(format!("<body><agentAddr>{}</agentAddr></body>", agent).as_ref()),
            &mut cseq,
            app_username,
            app_userkey,
        )
        .await?;

    socket2
        .connect(agent)
        .await
        .map_err(|e| format!("Could not reconnect to relay agent {agent}: {e}"))?;
    socket2.dh_read().await?;

    let mut session = PTCPSession::new();

    socket2.ptcp_request(session.send(PTCPBody::Sync)).await?;
    session.recv(socket2.ptcp_read().await?);

    if relay_mode {
        return Ok((socket2, session));
    }

    socket2
        .ptcp_request(session.send(PTCPBody::Command(
            b"\x17\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00".to_vec(),
        )))
        .await?;
    let mut res = session.recv(socket2.ptcp_read().await?);

    while let PTCPBody::Empty = res.body {
        res = session.recv(socket2.ptcp_read().await?);
    }

    let sign = match res.body {
        PTCPBody::Command(ref c) if c.len() >= 12 => &c[12..],
        PTCPBody::Command(_) => return Err("P2P sign response was truncated".to_string()),
        _ => return Err("P2P sign response had an unexpected packet type".to_string()),
    };

    println!(
        "Sign: {}",
        sign.iter()
            .map(|b| format!("{:02x}", b))
            .collect::<Vec<_>>()
            .join("")
    );

    let cookie: [u8; 4] = rand::random();
    let trans_id: [u8; 12] = rand::random();
    let cid: Vec<u8> = cid.iter().map(|b| !b).collect();

    let device_peer = socket
        .peer_addr()
        .map_err(|e| format!("Device peer is unavailable: {e}"))?;
    println!(">>> {}", device_peer);
    let data = [
        b"\xff\xfe\xff\xe7".to_vec(),
        cookie.to_vec(),
        trans_id.to_vec(),
        b"\x7f\xd5\xff\xf7".to_vec(),
        cid.clone(),
        b"\xff\xfb\xff\xf7\xff\xfe".to_vec(),
        ip_to_bytes(device)?,
    ]
    .concat();
    println!(
        "Raw [{}]",
        data.iter()
            .map(|b| format!("{:02x}", b))
            .collect::<Vec<_>>()
            .join(" ")
    );
    socket
        .send(&data)
        .await
        .map_err(|e| format!("Could not send device handshake: {e}"))?;
    println!("---");

    println!("<<< {}", device_peer);
    let mut buf = [0u8; 4096];

    let n = recv_with_timeout(
        &socket,
        &mut buf,
        DEVICE_HANDSHAKE_TIMEOUT,
        "the device handshake response",
    )
    .await
    .map_err(|e| {
        format!("{e} If the issue persists, relay mode may be required for this device.")
    })?;
    println!(
        "Raw [{}]",
        buf[0..n]
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<Vec<_>>()
            .join(" ")
    );
    println!("---");

    if n < 20 {
        return Err("Device handshake response was truncated".to_string());
    }
    let rtrans_id = &buf[8..20];

    println!(">>> {}", device_peer);
    let data = [
        b"\xfe\xfe\xff\xe7".to_vec(),
        cookie.to_vec(),
        rtrans_id.to_vec(),
        b"\x7f\xd6\xff\xf7".to_vec(),
        cid.clone(),
        b"\xff\xfb\xff\xf7\xff\xfe".to_vec(),
        ip_to_bytes(device_laddr)?,
    ]
    .concat();
    println!(
        "Raw [{}]",
        data.iter()
            .map(|b| format!("{:02x}", b))
            .collect::<Vec<_>>()
            .join(" ")
    );
    socket
        .send(&data)
        .await
        .map_err(|e| format!("Could not send device address handshake: {e}"))?;
    println!("---");

    // read 5 times
    for _ in 0..5 {
        println!("<<< {}", device_peer);
        let n = recv_with_timeout(
            &socket,
            &mut buf,
            DEVICE_HANDSHAKE_TIMEOUT,
            "a device address handshake packet",
        )
        .await?;
        println!(
            "Raw [{}]",
            buf[0..n]
                .iter()
                .map(|b| format!("{:02x}", b))
                .collect::<Vec<_>>()
                .join(" ")
        );
        println!("---");
    }

    let mut session = PTCPSession::new();

    socket.ptcp_request(session.send(PTCPBody::Sync)).await?;
    let mut res = session.recv(socket.ptcp_read().await?);
    if !matches!(res.body, PTCPBody::Sync) {
        return Err("Device returned an invalid PTCP sync response".to_string());
    }

    socket
        .ptcp_request(
            session.send(PTCPBody::Command(
                [
                    b"\x19\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00".to_vec(),
                    sign.to_vec(),
                ]
                .concat(),
            )),
        )
        .await?;

    res = session.recv(socket.ptcp_read().await?);
    while let PTCPBody::Empty = res.body {
        res = session.recv(socket.ptcp_read().await?);
    }
    match res.body {
        PTCPBody::Command(ref c) => {
            if c.first() != Some(&0x1A) {
                return Err("Device returned an invalid PTCP auth response".to_string());
            }
        }
        _ => return Err("Device returned an unexpected PTCP auth response".to_string()),
    }

    socket
        .ptcp_request(session.send(PTCPBody::Command(
            b"\x1b\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00".to_vec(),
        )))
        .await?;
    res = session.recv(socket.ptcp_read().await?);

    if !matches!(res.body, PTCPBody::Empty) {
        return Err("Device returned an invalid PTCP close response".to_string());
    }

    Ok((socket, session))
}

#[derive(Debug)]
#[allow(dead_code)]
struct DHResponse {
    version: String,
    code: u16,
    status: String,
    headers: HashMap<String, String>,
    body: Option<HashMap<String, String>>,
}

impl DHResponse {
    fn parse_body(body: &str) -> Result<HashMap<String, String>, String> {
        // XmlBody::Value("")
        let mut parser = EventReader::from_str(body);
        let mut stack = Vec::new();
        let mut tree = HashMap::new();

        loop {
            match parser.next() {
                Ok(XmlEvent::StartElement { name, .. }) => {
                    stack.push(name.local_name);
                }
                Ok(XmlEvent::EndElement { .. }) => {
                    stack.pop().ok_or_else(|| {
                        "Invalid DH XML body: unexpected closing element".to_string()
                    })?;
                }
                Ok(XmlEvent::Characters(s)) => {
                    let key = stack.as_slice().join("/");
                    tree.insert(key, s);
                }
                Ok(XmlEvent::EndDocument) => {
                    break;
                }
                Err(e) => return Err(format!("Invalid DH XML body: {e}")),
                _ => {}
            }
        }

        if !stack.is_empty() {
            return Err("Invalid DH XML body: unclosed element".to_string());
        }

        Ok(tree)
    }

    fn parse_response(res: &str) -> Result<DHResponse, String> {
        // split head and body by "\r\n\r\n"
        let (head, body) = res
            .split_once("\r\n\r\n")
            .ok_or_else(|| "Invalid DH response: header/body separator is missing".to_string())?;

        let mut head_parts = head.split("\r\n");
        let status_line = head_parts
            .next()
            .ok_or_else(|| "Invalid DH response: status line is missing".to_string())?;
        let mut status_parts = status_line.splitn(3, ' ');
        let version = status_parts
            .next()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "Invalid DH response: protocol version is missing".to_string())?
            .to_string();
        let code = status_parts
            .next()
            .ok_or_else(|| "Invalid DH response: status code is missing".to_string())?
            .parse::<u16>()
            .map_err(|e| format!("Invalid DH response status code: {e}"))?;
        let status = status_parts.next().unwrap_or_default().trim().to_string();

        let mut headers = HashMap::new();
        for line in head_parts {
            let (key, value) = line
                .split_once(':')
                .ok_or_else(|| format!("Invalid DH response header: {line}"))?;
            let key = key.trim();
            let value = value.trim();
            if key.is_empty() {
                return Err("Invalid DH response header: name is empty".to_string());
            }
            headers.insert(key.to_string(), value.to_string());
        }

        let body = match body.trim().len() {
            0 => None,
            _ => Some(DHResponse::parse_body(body)?),
        };

        Ok(DHResponse {
            version,
            code,
            status,
            headers,
            body,
        })
    }
}

#[async_trait]
trait DHP2P {
    async fn dh_request(
        &self,
        path: &str,
        body: Option<&str>,
        seq: &mut u32,
        username: &str,
        userkey: &str,
    ) -> Result<(), String>;
    async fn dh_read_raw(&self) -> Result<DHResponse, String>;

    async fn dh_read(&self) -> Result<DHResponse, String> {
        let res = self.dh_read_raw().await;

        let res = res?;
        if res.code >= 300 {
            return Err(format!("DH provider returned {} {}", res.code, res.status));
        }

        Ok(res)
    }
}

#[async_trait]
impl DHP2P for UdpSocket {
    async fn dh_request(
        &self,
        path: &str,
        body: Option<&str>,
        seq: &mut u32,
        username: &str,
        userkey: &str,
    ) -> Result<(), String> {
        let method = match body {
            Some(_) => "DHPOST",
            None => "DHGET",
        };

        let body = body.unwrap_or_default();

        let nonce = rand::random::<u32>();
        let currdate = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let pwd = format!("{}{}DHP2P:{}:{}", nonce, currdate, username, userkey);

        let mut hasher = sha1::Sha1::new();
        hasher.update(pwd);
        let hash_digest = hasher.finalize();
        let digest = base64::engine::general_purpose::STANDARD.encode(hash_digest);

        *seq += 1;

        let req = format!("\
            {} {} HTTP/1.1\r\n\
            CSeq: {}\r\n\
            Authorization: WSSE profile=\"UsernameToken\"\r\n\
            X-WSSE: UsernameToken Username=\"{}\", PasswordDigest=\"{}\", Nonce=\"{}\", Created=\"{}\"\r\n\r\n{}",
            method, path, seq, username, digest, nonce, currdate, body,
        );

        let peer = self
            .peer_addr()
            .map_err(|e| format!("DH peer is unavailable: {e}"))?;
        println!(">>> {} {} {} (CSeq {})", peer, method, path, seq);

        self.send(req.as_bytes())
            .await
            .map(|_| ())
            .map_err(|e| format!("Could not send DH request: {e}"))
    }

    async fn dh_read_raw(&self) -> Result<DHResponse, String> {
        let peer = self
            .peer_addr()
            .map_err(|e| format!("DH peer is unavailable: {e}"))?;
        println!("### {}", peer);

        let mut buf = [0u8; 4096];
        let n = time::timeout(DH_READ_TIMEOUT, self.recv(&mut buf))
            .await
            .map_err(|_| {
                format!(
                    "Timed out waiting {} seconds for DH response from {peer}",
                    DH_READ_TIMEOUT.as_secs()
                )
            })?
            .map_err(|e| format!("Could not read DH response: {e}"))?;
        let res = String::from_utf8_lossy(&buf[0..n]);

        println!("<<< DH {}", peer);

        let res = DHResponse::parse_response(&res)?;
        println!("DH response {} {}", res.code, res.status);

        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_dh_responses_return_errors() {
        assert!(DHResponse::parse_response("not a DH response").is_err());
        assert!(DHResponse::parse_response("HTTP/1.1 200 OK\r\n\r\n<broken").is_err());
        assert!(DHResponse::parse_response("HTTP/1.1 nope OK\r\n\r\n").is_err());
    }

    #[test]
    fn dh_response_keeps_nested_body_keys() {
        let response = DHResponse::parse_response(
            "HTTP/1.1 200 OK\r\nCSeq: 1\r\n\r\n<body><Address>127.0.0.1:8800</Address></body>",
        )
        .expect("valid DH response should parse");

        assert_eq!(response.code, 200);
        assert_eq!(
            response
                .body
                .as_ref()
                .and_then(|body| body.get("body/Address")),
            Some(&"127.0.0.1:8800".to_string())
        );
    }
}
