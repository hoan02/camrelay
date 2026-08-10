use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::net::{TcpListener, UdpSocket};
use tokio::sync::{mpsc, oneshot};

use crate::config::{load_brands, Camera};
use crate::dh::p2p_handshake;
use crate::process::{dh_reader, dh_writer, process_reader, process_writer};
use crate::ptcp::PTCPEvent;

#[derive(Clone, Debug)]
pub enum TunnelStatus {
    Stopped,
    Starting,
    Running,
    Error(String),
}

type StatusMap = Arc<Mutex<HashMap<String, TunnelStatus>>>;
type HandleMap = Arc<Mutex<HashMap<String, tokio::task::AbortHandle>>>;

pub struct TunnelManager {
    statuses: StatusMap,
    handles: HandleMap,
}

impl TunnelManager {
    pub fn new() -> Self {
        TunnelManager {
            statuses: Arc::new(Mutex::new(HashMap::new())),
            handles: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn status(&self, id: &str) -> TunnelStatus {
        self.statuses.lock().unwrap().get(id).cloned().unwrap_or(TunnelStatus::Stopped)
    }

    pub fn start(&self, camera: Camera) -> Result<(), String> {
        let id = camera.id.clone();

        match self.status(&id) {
            TunnelStatus::Running | TunnelStatus::Starting => {
                return Err("Tunnel already running".to_string());
            }
            _ => {}
        }

        let statuses = self.statuses.clone();
        let handles = self.handles.clone();
        let id_clone = id.clone();

        statuses.lock().unwrap().insert(id.clone(), TunnelStatus::Starting);

        let task = tokio::spawn(async move {
            let s = statuses.clone();
            let inner = tokio::spawn(run_tunnel(camera, s.clone()));

            match inner.await {
                Ok(Ok(_)) => { s.lock().unwrap().insert(id_clone.clone(), TunnelStatus::Stopped); }
                Ok(Err(e)) => { s.lock().unwrap().insert(id_clone.clone(), TunnelStatus::Error(e)); }
                Err(e) => { s.lock().unwrap().insert(id_clone.clone(), TunnelStatus::Error(format!("Panic: {}", e))); }
            }

            handles.lock().unwrap().remove(&id_clone);
        });

        self.handles.lock().unwrap().insert(id, task.abort_handle());
        Ok(())
    }

    pub fn stop(&self, id: &str) {
        if let Some(handle) = self.handles.lock().unwrap().remove(id) {
            handle.abort();
        }
        self.statuses.lock().unwrap().insert(id.to_string(), TunnelStatus::Stopped);
    }
}

async fn run_tunnel(camera: Camera, statuses: StatusMap) -> Result<(), String> {
    // Look up brand to get server/credentials
    let brands = load_brands();
    let brand = brands.into_iter().find(|b| b.name == camera.brand)
        .ok_or_else(|| format!("Brand '{}' not found in brands.json", camera.brand))?;

    let listener = TcpListener::bind(format!("0.0.0.0:{}", camera.local_port))
        .await
        .map_err(|e| format!("Port {} busy: {}", camera.local_port, e))?;

    let socket = UdpSocket::bind("0.0.0.0:0").await.map_err(|e| e.to_string())?;

    println!("[{}] Connecting via {} ({})...", camera.name, brand.name, brand.main_server);
    let (socket, session) = p2p_handshake(
        socket,
        camera.serial.clone(),
        false,
        &brand.main_server,
        &brand.app_username,
        &brand.app_userkey,
    ).await;

    let (dh_tx, dh_rx) = mpsc::channel::<PTCPEvent>(128);
    let session = Arc::new(Mutex::new(session));
    let channels = Arc::new(Mutex::new(HashMap::<u32, mpsc::Sender<Vec<u8>>>::new()));
    let conn_channels = Arc::new(Mutex::new(HashMap::<u32, oneshot::Sender<bool>>::new()));

    statuses.lock().unwrap().insert(camera.id.clone(), TunnelStatus::Running);

    println!("[{}] Ready — rtsp://{}:{}@127.0.0.1:{}/cam/realmonitor?channel=1&subtype=0",
        camera.name, camera.username, camera.password, camera.local_port);

    let reader = Arc::new(socket);
    let writer = reader.clone();
    let session2 = session.clone();
    let channels2 = channels.clone();
    let conn_channels2 = conn_channels.clone();
    let remote_port = camera.port as u32;

    let hb_tx = dh_tx.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            if hb_tx.send(PTCPEvent::Heartbeat).await.is_err() { break; }
        }
    });

    tokio::spawn(async move { dh_writer(session, writer, dh_rx, remote_port).await; });
    tokio::spawn(async move { dh_reader(session2, reader, channels, conn_channels).await; });

    loop {
        let (client, addr) = listener.accept().await.map_err(|e| e.to_string())?;
        println!("[{}] Connection from {}", camera.name, addr);

        let (tx, rx) = mpsc::channel::<Vec<u8>>(128);
        let (conn_tx, conn_rx) = oneshot::channel::<bool>();
        let dh_tx_clone = dh_tx.clone();

        let realm_id = rand::random::<u32>();
        channels2.lock().unwrap().insert(realm_id, tx);
        conn_channels2.lock().unwrap().insert(realm_id, conn_tx);

        dh_tx.send(PTCPEvent::Connect(realm_id)).await.map_err(|e| e.to_string())?;
        conn_rx.await.map_err(|e| e.to_string())?;

        let (r, w) = client.into_split();
        tokio::spawn(async move { process_reader(r, realm_id, dh_tx_clone).await; });
        tokio::spawn(async move { process_writer(w, rx).await; });
    }
}
