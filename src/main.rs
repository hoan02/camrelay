use std::sync::{Arc, Mutex};

use crate::config::AppConfig;
use crate::tunnel::TunnelManager;
use crate::web::{AppState, create_router};

mod config;
mod dh;
mod process;
mod ptcp;
mod tunnel;
mod web;

#[tokio::main]
async fn main() {
    let config = AppConfig::load();
    let web_port = config.web_port;

    let tunnel_manager = Arc::new(TunnelManager::new());

    // Auto-start cameras
    for camera in crate::config::load_cameras().into_iter().filter(|c| c.auto_start) {
        println!("Auto-starting [{}]...", camera.name);
        let _ = tunnel_manager.start(camera);
    }

    let state = AppState {
        config,
        sessions: Arc::new(Mutex::new(Vec::new())),
        tunnel_manager,
    };

    let app = create_router(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", web_port))
        .await
        .expect("Failed to bind web server port");

    println!("camrelay manager running at http://localhost:{}", web_port);

    axum::serve(listener, app).await.unwrap();
}
