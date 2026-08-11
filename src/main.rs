use std::sync::{Arc, Mutex};

use crate::config::AppConfig;
use crate::recordings::{archive_poll_interval, RecordingManager};
use crate::tunnel::TunnelManager;
use crate::web::{create_router, AppState};

mod config;
mod dh;
mod process;
mod ptcp;
mod recordings;
mod tunnel;
mod web;

#[tokio::main]
async fn main() {
    let config = AppConfig::load();
    let web_port = config.web_port;

    let tunnel_manager = Arc::new(TunnelManager::new());
    let recording_manager = RecordingManager::new(config.clone());

    // Auto-start cameras
    for camera in crate::config::load_cameras()
        .into_iter()
        .filter(|c| c.auto_start)
    {
        println!("Auto-starting [{}]...", camera.name);
        let _ = tunnel_manager.start(camera);
    }

    let recording_manager_task = recording_manager.clone();
    let tunnel_manager_task = tunnel_manager.clone();
    tokio::spawn(async move {
        let mut interval =
            tokio::time::interval(archive_poll_interval(recording_manager_task.config()));
        loop {
            interval.tick().await;
            recording_manager_task
                .reconcile(&crate::config::load_cameras(), &tunnel_manager_task)
                .await;
        }
    });

    let state = AppState {
        config,
        sessions: Arc::new(Mutex::new(Vec::new())),
        tunnel_manager,
        recording_manager,
        playback_tickets: Arc::new(Mutex::new(std::collections::HashMap::new())),
    };

    let app = create_router(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", web_port))
        .await
        .expect("Failed to bind web server port");

    println!("camrelay manager running at http://localhost:{}", web_port);

    axum::serve(listener, app).await.unwrap();
}
