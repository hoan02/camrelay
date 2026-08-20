use async_trait::async_trait;
use std::cmp;
use tokio::{net::UdpSocket, time};

const PTCP_READ_TIMEOUT: time::Duration = time::Duration::from_secs(8);

pub enum PTCPEvent {
    Heartbeat,
    Connect(u32),
    Disconnect(u32),
    Data(u32, Vec<u8>),
}

pub struct PTCPPayload {
    pub realm: u32,
    pub data: Vec<u8>,
}

pub enum PTCPBody {
    Sync,
    Command(Vec<u8>),
    Payload(PTCPPayload),
    Bind(u32, u32),
    Status(u32, String),
    Heartbeat,
    Empty,
}

pub struct PTCPPacket {
    sent: u32,
    recv: u32,
    pid: u32,
    lmid: u32,
    rmid: u32,
    pub body: PTCPBody,
}

impl PTCPPayload {
    fn parse(data: &[u8]) -> Result<PTCPPayload, String> {
        if data.len() < 12 {
            return Err("Invalid PTCP payload: header is truncated".to_string());
        }
        if data[0] != 0x10 {
            return Err("Invalid PTCP payload header".to_string());
        }

        // first 4 bytes it header
        let header = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        let length = header & 0xFFFF;
        let realm = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        let padding = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);
        let data = data[12..].to_vec();

        if padding != 0 {
            return Err("Invalid PTCP payload padding".to_string());
        }
        if length != data.len() as u32 {
            return Err(format!(
                "Invalid PTCP payload length: header={length}, actual={}",
                data.len()
            ));
        }

        Ok(PTCPPayload { realm, data })
    }

    fn serialize(&self) -> Vec<u8> {
        let length = self.data.len() as u32;
        let header = 0x10000000 | length;
        let header = header.to_be_bytes();
        let realm = self.realm.to_be_bytes();
        let padding = 0u32.to_be_bytes();

        [
            header.to_vec(),
            realm.to_vec(),
            padding.to_vec(),
            self.data.clone(),
        ]
        .concat()
    }
}

impl std::fmt::Debug for PTCPPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "length: {}, realm: 0x{:08x}, data: [{}{}]",
            self.data.len(),
            self.realm,
            self.data[0..cmp::min(self.data.len(), 16)]
                .iter()
                .map(|b| format!("{:02x}", b))
                .collect::<Vec<_>>()
                .join(" "),
            if self.data.len() > 16 { " ..." } else { "" },
        )
    }
}

impl std::fmt::Debug for PTCPBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PTCPBody::Sync => write!(f, "Sync"),
            PTCPBody::Command(data) => write!(
                f,
                "Command([{}])",
                data.iter()
                    .map(|b| format!("{:02x}", b))
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
            PTCPBody::Payload(payload) => write!(f, "{:?}", payload),
            PTCPBody::Bind(realm, port) => {
                write!(f, "Bind {{ realm: 0x{:08x}, port: {} }}", realm, port)
            }
            PTCPBody::Status(realm, status) => {
                write!(f, "Status {{ realm: 0x{:08x}, status: {} }}", realm, status)
            }
            PTCPBody::Heartbeat => write!(f, "Heartbeat"),
            PTCPBody::Empty => write!(f, "Empty"),
        }
    }
}

impl std::fmt::Debug for PTCPPacket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "PTCPPacket {{ sent: {}, recv: {}, pid: 0x{:08x}, lmid: 0x{:08x}, rmid: 0x{:08x}, body: {:?} }}",
            self.sent, self.recv, self.pid, self.lmid, self.rmid, self.body
        )
    }
}

impl PTCPBody {
    fn parse(data: &[u8]) -> Result<PTCPBody, String> {
        if data.is_empty() {
            return Ok(PTCPBody::Empty);
        }

        if data.len() < 4 {
            return Err("Invalid PTCP body: header is truncated".to_string());
        }

        match data[0] {
            0x00 => Ok(PTCPBody::Sync),
            0x10 => Ok(PTCPBody::Payload(PTCPPayload::parse(data)?)),
            0x11 => {
                if data.len() < 20 {
                    return Err("Invalid PTCP bind body: body is truncated".to_string());
                }
                Ok(PTCPBody::Bind(
                    u32::from_be_bytes([data[4], data[5], data[6], data[7]]),
                    u32::from_be_bytes([data[12], data[13], data[14], data[15]]),
                ))
            }
            0x12 => {
                if data.len() < 12 {
                    return Err("Invalid PTCP status body: body is truncated".to_string());
                }
                let realm = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
                let status = String::from_utf8_lossy(&data[12..]).to_string();
                Ok(PTCPBody::Status(realm, status))
            }
            0x13 => Ok(PTCPBody::Heartbeat),
            _ => Ok(PTCPBody::Command(data.to_vec())),
        }
    }

    fn serialize(&self) -> Vec<u8> {
        match self {
            PTCPBody::Sync => b"\x00\x03\x01\x00".to_vec(),
            PTCPBody::Command(data) => data.to_vec(),
            PTCPBody::Payload(payload) => payload.serialize(),
            PTCPBody::Bind(realm, port) => [
                b"\x11\x00\x00\x00".to_vec(),
                realm.to_be_bytes().to_vec(),
                b"\x00\x00\x00\x00".to_vec(),
                port.to_be_bytes().to_vec(),
                b"\x7f\x00\x00\x01".to_vec(),
            ]
            .concat(),
            PTCPBody::Status(realm, status) => [
                b"\x12\x00\x00\x00".to_vec(),
                realm.to_be_bytes().to_vec(),
                b"\x00\x00\x00\x00".to_vec(),
                status.as_bytes().to_vec(),
            ]
            .concat(),
            PTCPBody::Heartbeat => b"\x13\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00".to_vec(),
            PTCPBody::Empty => Vec::new(),
        }
    }

    fn len(&self) -> usize {
        match self {
            PTCPBody::Sync => 4,
            PTCPBody::Command(data) => data.len(),
            PTCPBody::Payload(payload) => payload.data.len() + 12,
            PTCPBody::Bind(_, _) => 20,
            PTCPBody::Status(_, status) => status.len() + 12,
            PTCPBody::Heartbeat => 12,
            PTCPBody::Empty => 0,
        }
    }
}

impl PTCPPacket {
    fn parse(data: &[u8]) -> Result<PTCPPacket, String> {
        if data.len() < 24 {
            return Err("Invalid PTCP packet: header is truncated".to_string());
        }

        let magic = &data[0..4];

        if magic != b"PTCP" {
            return Err("Invalid PTCP packet magic".to_string());
        }

        let sent = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        let recv = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);
        let pid = u32::from_be_bytes([data[12], data[13], data[14], data[15]]);
        let lmid = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
        let rmid = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
        let body = PTCPBody::parse(&data[24..])?;

        Ok(PTCPPacket {
            sent,
            recv,
            pid,
            lmid,
            rmid,
            body,
        })
    }

    fn serialize(&self) -> Vec<u8> {
        [
            b"PTCP".to_vec(),
            self.sent.to_be_bytes().to_vec(),
            self.recv.to_be_bytes().to_vec(),
            self.pid.to_be_bytes().to_vec(),
            self.lmid.to_be_bytes().to_vec(),
            self.rmid.to_be_bytes().to_vec(),
            self.body.serialize(),
        ]
        .concat()
    }
}

pub struct PTCPSession {
    sent: u32,
    recv: u32,
    count: u32,
    id: u32,
    rmid: u32,
}

impl PTCPSession {
    pub fn new() -> PTCPSession {
        PTCPSession {
            sent: 0,
            recv: 0,
            count: 0,
            id: 0,
            rmid: 0,
        }
    }

    pub fn send(&mut self, body: PTCPBody) -> PTCPPacket {
        let sent = self.sent;
        let recv = self.recv;
        let pid = match body {
            PTCPBody::Sync => 0x0002FFFF,
            _ => 0x0000FFFF - self.count,
        };
        let lmid = self.id;
        let rmid = self.rmid;

        /*
         * Update counters
         */
        self.sent += body.len() as u32;

        self.id += 1;
        self.count += match body {
            PTCPBody::Sync => 0,
            PTCPBody::Empty => 0,
            _ => 1,
        };

        PTCPPacket {
            sent,
            recv,
            pid,
            lmid,
            rmid,
            body,
        }
    }

    pub fn recv(&mut self, packet: PTCPPacket) -> PTCPPacket {
        self.recv += packet.body.len() as u32;
        self.rmid = packet.lmid;

        packet
    }
}

#[async_trait]
pub trait Ptcp {
    async fn ptcp_request(&self, packet: PTCPPacket) -> Result<(), String>;
    async fn ptcp_read(&self) -> Result<PTCPPacket, String>;
}

#[async_trait]
impl Ptcp for UdpSocket {
    async fn ptcp_request(&self, packet: PTCPPacket) -> Result<(), String> {
        let peer = self
            .peer_addr()
            .map_err(|e| format!("PTCP peer is unavailable: {e}"))?;
        println!(">>> PTCP {}", peer);

        let packet = packet.serialize();
        self.send(&packet)
            .await
            .map(|_| ())
            .map_err(|e| format!("Could not send PTCP packet: {e}"))
    }

    async fn ptcp_read(&self) -> Result<PTCPPacket, String> {
        let peer = self
            .peer_addr()
            .map_err(|e| format!("PTCP peer is unavailable: {e}"))?;
        println!("### {}", peer);

        let mut buf = [0u8; 4096];
        let n = time::timeout(PTCP_READ_TIMEOUT, self.recv(&mut buf))
            .await
            .map_err(|_| {
                format!(
                    "Timed out waiting {} seconds for PTCP packet from {peer}",
                    PTCP_READ_TIMEOUT.as_secs()
                )
            })?
            .map_err(|e| format!("Could not read PTCP packet: {e}"))?;

        println!("<<< PTCP {}", peer);
        let packet = PTCPPacket::parse(&buf[0..n])?;

        Ok(packet)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_packets_return_errors_instead_of_panicking() {
        assert!(PTCPPacket::parse(b"not-a-packet").is_err());

        let mut truncated_body = b"PTCP".to_vec();
        truncated_body.extend([0; 20]);
        truncated_body.extend([0x10, 0x00, 0x00]);
        assert!(PTCPPacket::parse(&truncated_body).is_err());

        let mut truncated_bind = b"PTCP".to_vec();
        truncated_bind.extend([0; 20]);
        truncated_bind.extend([0x11, 0x00, 0x00, 0x00]);
        assert!(PTCPPacket::parse(&truncated_bind).is_err());
    }

    #[test]
    fn valid_sync_packet_still_round_trips() {
        let packet = PTCPPacket {
            sent: 0,
            recv: 0,
            pid: 0x0002FFFF,
            lmid: 0,
            rmid: 0,
            body: PTCPBody::Sync,
        };

        let parsed = PTCPPacket::parse(&packet.serialize()).expect("sync packet should parse");
        assert!(matches!(parsed.body, PTCPBody::Sync));
    }
}
