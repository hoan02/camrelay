use std::{
    collections::HashMap,
    fs,
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration as StdDuration, SystemTime},
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::{
    process::{Child, Command},
    time::Duration,
};
use uuid::Uuid;

use crate::{
    config::{AppConfig, Camera},
    rtsp_proxy::RtspProxyManager,
    tunnel::{TunnelManager, TunnelStatus},
};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Recording {
    pub id: String,
    pub camera_id: String,
    pub camera_name: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub kind: String,
    pub local_path: String,
    pub archive_path: Option<String>,
    pub drive_file_id: Option<String>,
    pub bytes: u64,
    pub status: String,
    pub error: Option<String>,
}

#[derive(Clone)]
pub struct RecordingManager {
    config: AppConfig,
    proxy: RtspProxyManager,
    records: Arc<Mutex<Vec<Recording>>>,
    processes: Arc<Mutex<HashMap<String, Child>>>,
}

impl RecordingManager {
    pub fn new(config: AppConfig, proxy: RtspProxyManager) -> Self {
        let records = fs::read_to_string(&config.recordings_index)
            .ok()
            .and_then(|content| serde_json::from_str::<Vec<Recording>>(&content).ok())
            .unwrap_or_default();
        Self {
            config,
            proxy,
            records: Arc::new(Mutex::new(records)),
            processes: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn list(&self) -> Vec<Recording> {
        let mut records = self.records.lock().unwrap().clone();
        records.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        records
    }

    pub fn list_filtered(
        &self,
        camera_id: Option<&str>,
        status: Option<&str>,
        limit: usize,
    ) -> Vec<Recording> {
        self.list()
            .into_iter()
            .filter(|record| {
                camera_id
                    .map(|value| value.is_empty() || record.camera_id == value)
                    .unwrap_or(true)
            })
            .filter(|record| {
                status
                    .map(|value| value.is_empty() || record.status == value)
                    .unwrap_or(true)
            })
            .take(limit.clamp(1, 500))
            .collect()
    }

    pub fn get(&self, id: &str) -> Option<Recording> {
        self.records
            .lock()
            .unwrap()
            .iter()
            .find(|record| record.id == id)
            .cloned()
    }

    pub fn config(&self) -> &AppConfig {
        &self.config
    }

    pub async fn reconcile(&self, cameras: &[Camera], tunnels: &TunnelManager) {
        self.reap_finished();
        self.stop_inactive(cameras, tunnels);
        self.index_closed_files(cameras);

        if self.config.recordings_enabled {
            for camera in cameras
                .iter()
                .filter(|camera| matches!(tunnels.status(&camera.id), TunnelStatus::Running))
            {
                self.start_recorder(camera).await;
            }
        }

        if self.config.archive_enabled && !self.config.archive_remote.trim().is_empty() {
            self.schedule_uploads();
        }
    }

    pub async fn archive(&self, id: &str) -> Result<Recording, String> {
        let record = self
            .get(id)
            .ok_or_else(|| "Recording not found".to_string())?;
        if record.status == "archived" {
            return Ok(record);
        }
        self.set_status(id, "uploading", None);
        self.upload(record).await?;
        self.get(id)
            .ok_or_else(|| "Recording disappeared after upload".to_string())
    }

    pub fn archive_target(&self, record: &Recording) -> Option<String> {
        let archive_path = record.archive_path.as_ref()?;
        let remote = self.config.archive_remote.trim().trim_end_matches('/');
        if remote.is_empty() {
            None
        } else {
            Some(format!("{}/{}", remote, archive_path))
        }
    }

    fn persist(&self) {
        let records = self.records.lock().unwrap().clone();
        if let Ok(content) = serde_json::to_string_pretty(&records) {
            let _ = fs::write(&self.config.recordings_index, content);
        }
    }

    fn set_status(&self, id: &str, status: &str, error: Option<String>) {
        if let Some(record) = self
            .records
            .lock()
            .unwrap()
            .iter_mut()
            .find(|record| record.id == id)
        {
            record.status = status.to_string();
            record.error = error;
        }
        self.persist();
    }

    fn reap_finished(&self) {
        let mut processes = self.processes.lock().unwrap();
        let finished: Vec<String> = processes
            .iter_mut()
            .filter_map(|(id, child)| child.try_wait().ok().flatten().map(|_| id.clone()))
            .collect();
        for id in finished {
            processes.remove(&id);
        }
    }

    fn stop_inactive(&self, cameras: &[Camera], tunnels: &TunnelManager) {
        let active: std::collections::HashSet<String> = cameras
            .iter()
            .filter(|camera| matches!(tunnels.status(&camera.id), TunnelStatus::Running))
            .map(|camera| camera.id.clone())
            .collect();
        let mut processes = self.processes.lock().unwrap();
        let stopped: Vec<String> = processes
            .keys()
            .filter(|id| !active.contains(*id))
            .cloned()
            .collect();
        for id in stopped {
            if let Some(mut child) = processes.remove(&id) {
                let _ = child.start_kill();
            }
            self.proxy.stop(&id);
        }
    }

    async fn start_recorder(&self, camera: &Camera) {
        if self.processes.lock().unwrap().contains_key(&camera.id) {
            return;
        }

        let proxy_port = match self.proxy.ensure(camera.clone()).await {
            Ok(port) => port,
            Err(error) => {
                println!(
                    "[{}] Could not start RTSP credential proxy: {}",
                    camera.name, error
                );
                return;
            }
        };
        let mut processes = self.processes.lock().unwrap();

        let camera_dir = Path::new(&self.config.recordings_dir).join(&camera.id);
        if fs::create_dir_all(&camera_dir).is_err() {
            self.proxy.stop(&camera.id);
            return;
        }
        let output = camera_dir.join("%Y-%m-%dT%H-%M-%S.mp4");
        let url = format!("rtsp://127.0.0.1:{proxy_port}/cam/realmonitor?channel=1&subtype=0");
        let segment_seconds = self.config.segment_seconds.max(30).to_string();
        let output = output.to_string_lossy().to_string();
        let result = Command::new(&self.config.ffmpeg_path)
            .args([
                "-hide_banner",
                "-loglevel",
                "warning",
                "-rtsp_transport",
                "tcp",
                "-i",
                &url,
                "-map",
                "0",
                "-c",
                "copy",
                "-f",
                "segment",
                "-segment_time",
                &segment_seconds,
                "-reset_timestamps",
                "1",
                "-strftime",
                "1",
                &output,
            ])
            .spawn();
        match result {
            Ok(child) => {
                println!(
                    "[{}] Recording started with {} second segments",
                    camera.name, segment_seconds
                );
                processes.insert(camera.id.clone(), child);
            }
            Err(error) => println!(
                "[{}] Could not start ffmpeg recorder: {}",
                camera.name, error
            ),
        }
        if !processes.contains_key(&camera.id) {
            self.proxy.stop(&camera.id);
        }
    }

    fn index_closed_files(&self, cameras: &[Camera]) {
        let camera_map: HashMap<&str, &str> = cameras
            .iter()
            .map(|camera| (camera.id.as_str(), camera.name.as_str()))
            .collect();
        let root = Path::new(&self.config.recordings_dir);
        let Ok(camera_dirs) = fs::read_dir(root) else {
            return;
        };
        let mut changed = false;
        let mut records = self.records.lock().unwrap();

        for camera_dir in camera_dirs.flatten().filter(|entry| entry.path().is_dir()) {
            let camera_id = camera_dir.file_name().to_string_lossy().to_string();
            let camera_name = camera_map
                .get(camera_id.as_str())
                .copied()
                .unwrap_or(camera_id.as_str())
                .to_string();
            let Ok(files) = fs::read_dir(camera_dir.path()) else {
                continue;
            };
            for file in files.flatten().filter(|entry| {
                entry.path().extension().and_then(|ext| ext.to_str()) == Some("mp4")
            }) {
                let path = file.path();
                let path_string = path.to_string_lossy().to_string();
                if records
                    .iter()
                    .any(|record| record.local_path == path_string)
                {
                    continue;
                }
                let Ok(metadata) = fs::metadata(&path) else {
                    continue;
                };
                if metadata.len() == 0
                    || metadata
                        .modified()
                        .ok()
                        .and_then(|time| time.elapsed().ok())
                        .unwrap_or(StdDuration::from_secs(0))
                        < StdDuration::from_secs(8)
                {
                    continue;
                }
                let modified = metadata.modified().unwrap_or(SystemTime::now());
                let ended_at: DateTime<Utc> = modified.into();
                let started_at = ended_at
                    - chrono::Duration::seconds(self.config.segment_seconds.max(30) as i64);
                let archive_path = Some(format!(
                    "{}/{}/{}",
                    self.config.archive_root.trim_matches('/'),
                    camera_id,
                    file.file_name().to_string_lossy()
                ));
                records.push(Recording {
                    id: Uuid::new_v4().to_string(),
                    camera_id: camera_id.clone(),
                    camera_name: camera_name.clone(),
                    started_at: started_at.to_rfc3339(),
                    ended_at: Some(ended_at.to_rfc3339()),
                    kind: "continuous".to_string(),
                    local_path: path_string,
                    archive_path,
                    drive_file_id: None,
                    bytes: metadata.len(),
                    status: "local".to_string(),
                    error: None,
                });
                changed = true;
            }
        }
        drop(records);
        if changed {
            self.persist();
        }
    }

    fn schedule_uploads(&self) {
        let candidates = {
            let mut records = self.records.lock().unwrap();
            let mut candidates = Vec::new();
            for record in records.iter_mut().filter(|record| record.status == "local") {
                if Path::new(&record.local_path).is_file() {
                    record.status = "uploading".to_string();
                    record.error = None;
                    candidates.push(record.clone());
                }
            }
            candidates
        };
        if candidates.is_empty() {
            return;
        }
        self.persist();
        for record in candidates {
            let manager = self.clone();
            tokio::spawn(async move {
                let _ = manager.upload(record).await;
            });
        }
    }

    async fn upload(&self, record: Recording) -> Result<(), String> {
        let target = match self.archive_target(&record) {
            Some(target) => target,
            None => {
                let error = "Archive remote is not configured".to_string();
                self.set_status(&record.id, "failed", Some(error.clone()));
                return Err(error);
            }
        };
        if !Path::new(&record.local_path).is_file() {
            let error = "Local recording file is missing".to_string();
            self.set_status(&record.id, "failed", Some(error.clone()));
            return Err(error);
        }
        let output = match Command::new("rclone")
            .args(["copyto", &record.local_path, &target])
            .output()
            .await
        {
            Ok(output) => output,
            Err(error) => {
                let message = format!("Could not start rclone: {}", error);
                self.set_status(&record.id, "failed", Some(message.clone()));
                return Err(message);
            }
        };
        if output.status.success() {
            self.set_status(&record.id, "archived", None);
            Ok(())
        } else {
            let error = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let error = if error.is_empty() {
                format!("rclone exited with {}", output.status)
            } else {
                error
            };
            self.set_status(&record.id, "failed", Some(error.clone()));
            Err(error)
        }
    }
}

pub fn archive_poll_interval(config: &AppConfig) -> Duration {
    Duration::from_secs(config.archive_poll_seconds.max(5))
}
