use std::path::Path;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use crate::config::{AppConfig, Brand, Camera};
use crate::recordings::{archive_poll_interval, RecordingManager};
use crate::tunnel::TunnelManager;
use crate::web::{create_router, AppState};
use camrelay_storage::{Storage, StoredCameraConfig, StoredProviderConfig};

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

    let storage = if config.database_enabled {
        match Storage::open(&config.database_path).await {
            Ok(storage) => {
                if Path::new("config.json").exists() {
                    match Storage::load_legacy_snapshot(".") {
                        Ok(snapshot) => match storage.import_legacy(&snapshot).await {
                            Ok(report) => println!(
                                "Imported legacy JSON: users={}, providers={}, cameras={}, tokens={}",
                                report.users, report.providers, report.cameras, report.tokens
                            ),
                            Err(error) => eprintln!("Legacy JSON import failed: {error}"),
                        },
                        Err(error) => eprintln!("Could not read legacy JSON for import: {error}"),
                    }
                }
                Some(storage)
            }
            Err(error) => {
                eprintln!("SQLite storage disabled after startup error: {error}");
                None
            }
        }
    } else {
        None
    };

    let startup_cameras = match &storage {
        Some(storage) => match storage.list_camera_configs().await {
            Ok(cameras) => cameras.into_iter().map(camera_from_storage).collect(),
            Err(error) => {
                eprintln!("Could not read SQLite cameras; using legacy JSON: {error}");
                crate::config::load_cameras()
            }
        },
        None => crate::config::load_cameras(),
    };

    // Auto-start cameras
    for camera in startup_cameras.into_iter().filter(|c| c.auto_start) {
        println!("Auto-starting [{}]...", camera.name);
        let provider = match &storage {
            Some(storage) => storage
                .find_provider_config(&camera.brand)
                .await
                .ok()
                .flatten()
                .map(brand_from_storage),
            None => None,
        };
        let _ = match provider {
            Some(provider) => tunnel_manager.start_with_brand(camera, Some(provider)),
            None => tunnel_manager.start(camera),
        };
    }

    let recording_manager_task = recording_manager.clone();
    let tunnel_manager_task = tunnel_manager.clone();
    let storage_for_reconcile = storage.clone();
    tokio::spawn(async move {
        let mut interval =
            tokio::time::interval(archive_poll_interval(recording_manager_task.config()));
        loop {
            interval.tick().await;
            let cameras = match &storage_for_reconcile {
                Some(storage) => storage
                    .list_camera_configs()
                    .await
                    .map(|cameras| cameras.into_iter().map(camera_from_storage).collect())
                    .unwrap_or_else(|_| crate::config::load_cameras()),
                None => crate::config::load_cameras(),
            };
            recording_manager_task
                .reconcile(&cameras, &tunnel_manager_task)
                .await;
        }
    });

    let state = AppState {
        config,
        sessions: Arc::new(Mutex::new(HashMap::new())),
        tunnel_manager,
        recording_manager,
        playback_tickets: Arc::new(Mutex::new(std::collections::HashMap::new())),
        storage,
    };

    let app = create_router(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", web_port))
        .await
        .expect("Failed to bind web server port");

    println!("camrelay manager running at http://localhost:{}", web_port);

    axum::serve(listener, app).await.unwrap();
}

fn camera_from_storage(camera: StoredCameraConfig) -> Camera {
    Camera {
        id: camera.id,
        name: camera.name,
        brand: camera.brand,
        serial: camera.serial,
        username: camera.username,
        password: camera.password,
        port: camera.port,
        local_port: camera.local_port,
        auto_start: camera.auto_start,
    }
}

fn brand_from_storage(provider: StoredProviderConfig) -> Brand {
    Brand {
        id: provider.id,
        name: provider.name,
        main_server: provider.main_server,
        app_username: provider.app_username,
        app_userkey: provider.app_userkey,
    }
}
