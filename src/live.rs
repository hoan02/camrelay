//! Local HLS live gateway.
//!
//! FFmpeg reads a loopback RTSP endpoint exposed by `RtspProxyManager`, so
//! camera credentials never appear in this process command line. HLS files
//! are short-lived segments under the configured live directory and are
//! served through expiring, scoped tickets by the HTTP layer.

pub const HLS_PROTOCOL: &str = "hls";
pub const WEBRTC_PROTOCOL: &str = "webrtc";
const MEDIAMTX_RTSP_BASE_URL: &str = "rtsp://mediamtx:8554";

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

    /// Returns the configured transport name in the stable form used by the
    /// API ticket envelope. The string remains forward-compatible so a future
    /// gateway can add WebRTC without changing the client-facing shape.
    pub fn protocol(&self) -> String {
        let protocol = self.config.live_protocol.trim();
        if protocol.is_empty() {
            HLS_PROTOCOL.to_string()
        } else {
            protocol.to_ascii_lowercase()
        }
    }

    pub fn protocol_available(&self) -> bool {
        match self.protocol().as_str() {
            HLS_PROTOCOL => true,
            WEBRTC_PROTOCOL => self.config.webrtc_enabled,
            _ => false,
        }
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
                "Live media is disabled in config.json (set live_enabled to true).".to_string(),
            );
        }
        if !self.protocol_available() {
            return Err(format!(
                "Live protocol '{}' is not available; use 'hls' until another gateway is implemented.",
                self.protocol()
            ));
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
        let source = format!("rtsp://127.0.0.1:{proxy_port}/cam/realmonitor?channel=1&subtype=0");
        let child = if self.protocol() == WEBRTC_PROTOCOL {
            let output = format!("{MEDIAMTX_RTSP_BASE_URL}/camrelay/{}", camera.id);
            Command::new(&self.config.ffmpeg_path)
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
                    "-c:v",
                    "copy",
                    "-an",
                    "-f",
                    "rtsp",
                    "-rtsp_transport",
                    "tcp",
                    &output,
                ])
                .spawn()
        } else {
            let playlist = camera_dir.join("index.m3u8");
            let segment_pattern = camera_dir.join("segment-%05d.ts");
            let playlist = playlist.to_string_lossy().to_string();
            let segment_pattern = segment_pattern.to_string_lossy().to_string();
            Command::new(&self.config.ffmpeg_path)
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
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_live_gateway_is_hls() {
        let manager = LiveManager::new(AppConfig::default(), RtspProxyManager::new());

        assert_eq!(manager.protocol(), HLS_PROTOCOL);
        assert!(manager.protocol_available());
    }

    #[test]
    fn unsupported_live_gateway_is_reported_without_fallback() {
        let config = AppConfig {
            live_protocol: "webrtc".to_string(),
            ..AppConfig::default()
        };
        let manager = LiveManager::new(config, RtspProxyManager::new());

        assert_eq!(manager.protocol(), "webrtc");
        assert!(!manager.protocol_available());
    }

    #[test]
    fn webrtc_requires_explicit_configuration() {
        let config = AppConfig {
            live_protocol: WEBRTC_PROTOCOL.to_string(),
            webrtc_enabled: true,
            ..AppConfig::default()
        };
        let manager = LiveManager::new(config, RtspProxyManager::new());

        assert!(manager.protocol_available());
    }
}
