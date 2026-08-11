//! Local HLS live gateway.
//!
//! FFmpeg reads a loopback RTSP endpoint exposed by `RtspProxyManager`, so
//! camera credentials never appear in this process command line. HLS files
//! are short-lived segments under the configured live directory and are
//! served through expiring, scoped tickets by the HTTP layer.

use std::{
    collections::HashMap,
    fs,
    path::Path,
    sync::{Arc, Mutex},
};
use tokio::process::{Child, Command};

use crate::{
    config::{AppConfig, Camera},
    rtsp_proxy::RtspProxyManager,
};

#[derive(Clone)]
pub struct LiveManager {
    config: AppConfig,
    proxy: RtspProxyManager,
    processes: Arc<Mutex<HashMap<String, Child>>>,
}

impl LiveManager {
    pub fn new(config: AppConfig, proxy: RtspProxyManager) -> Self {
        Self {
            config,
            proxy,
            processes: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn config(&self) -> &AppConfig {
        &self.config
    }

    pub fn is_running(&self, camera_id: &str) -> bool {
        let mut processes = self.processes.lock().unwrap();
        let Some(child) = processes.get_mut(camera_id) else {
            return false;
        };
        match child.try_wait() {
            Ok(None) => true,
            Ok(Some(_)) | Err(_) => {
                processes.remove(camera_id);
                false
            }
        }
    }

    pub async fn start(&self, camera: Camera) -> Result<(), String> {
        if !self.config.live_enabled {
            return Err(
                "Live HLS is disabled in config.json (set live_enabled to true).".to_string(),
            );
        }
        if !safe_component(&camera.id) {
            return Err("Camera id is not safe for a live segment directory.".to_string());
        }
        if self.is_running(&camera.id) {
            return Ok(());
        }

        let proxy_port = self.proxy.ensure(camera.clone()).await?;
        let camera_dir = Path::new(&self.config.live_dir).join(&camera.id);
        if let Err(error) = fs::create_dir_all(&camera_dir) {
            self.proxy.stop(&camera.id);
            return Err(format!("Could not create live directory: {error}"));
        }
        let playlist = camera_dir.join("index.m3u8");
        let segment_pattern = camera_dir.join("segment-%05d.ts");
        let source = format!("rtsp://127.0.0.1:{proxy_port}/cam/realmonitor?channel=1&subtype=0");
        let playlist = playlist.to_string_lossy().to_string();
        let segment_pattern = segment_pattern.to_string_lossy().to_string();

        let child = Command::new(&self.config.ffmpeg_path)
            .args([
                "-hide_banner",
                "-loglevel",
                "warning",
                "-rtsp_transport",
                "tcp",
                "-i",
                &source,
                "-map",
                "0:v:0",
                "-map",
                "0:a:0?",
                "-c:v",
                "copy",
                "-c:a",
                "aac",
                "-f",
                "hls",
                "-hls_time",
                "2",
                "-hls_list_size",
                "6",
                "-hls_flags",
                "delete_segments+append_list+omit_endlist",
                "-hls_segment_filename",
                &segment_pattern,
                &playlist,
            ])
            .spawn()
            .map_err(|error| {
                self.proxy.stop(&camera.id);
                format!("Could not start FFmpeg live gateway: {error}")
            })?;
        self.processes.lock().unwrap().insert(camera.id, child);
        Ok(())
    }

    pub fn stop(&self, camera_id: &str) {
        if let Some(mut child) = self.processes.lock().unwrap().remove(camera_id) {
            let _ = child.start_kill();
        }
        self.proxy.stop(camera_id);
    }
}

fn safe_component(value: &str) -> bool {
    !value.is_empty()
        && !value.contains('/')
        && !value.contains('\\')
        && value != "."
        && value != ".."
}
