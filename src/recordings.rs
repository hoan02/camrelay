use std::{
    collections::HashMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration as StdDuration, SystemTime},
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
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
use camrelay_contract::RetentionPreview;

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
    #[serde(default)]
    pub checksum_sha256: Option<String>,
    #[serde(default)]
    pub archive_verified: bool,
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

    /// Calculates a safe, read-only retention preview. Deletion is deliberately
    /// not part of this method; an operations policy must explicitly define
    /// archive guarantees and an operator-approved cleanup action first.
    pub fn retention_preview(&self) -> RetentionPreview {
        let configured_days = self.config.local_retention_days;
        if configured_days == 0 {
            return RetentionPreview {
                configured_days,
                auto_delete_enabled: false,
                eligible_count: 0,
                eligible_bytes: 0,
                blocked_unarchived_count: 0,
                oldest_eligible_at: None,
            };
        }

        let cutoff = Utc::now() - chrono::Duration::days(i64::from(configured_days));
        let mut eligible_count = 0;
        let mut eligible_bytes: u64 = 0;
        let mut blocked_unarchived_count = 0;
        let mut oldest_eligible: Option<(DateTime<Utc>, String)> = None;

        for record in self.list() {
            if !matches!(record.status.as_str(), "local" | "archived") {
                continue;
            }
            let Some(ended_at) = record
                .ended_at
                .as_deref()
                .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
                .map(|value| value.with_timezone(&Utc))
            else {
                continue;
            };
            if ended_at >= cutoff || !Path::new(&record.local_path).is_file() {
                continue;
            }
            if self.config.archive_enabled && record.status != "archived" {
                blocked_unarchived_count += 1;
                continue;
            }
            eligible_count += 1;
            eligible_bytes = eligible_bytes.saturating_add(record.bytes);
            if oldest_eligible
                .as_ref()
                .map(|(oldest, _)| ended_at < *oldest)
                .unwrap_or(true)
            {
                oldest_eligible = Some((ended_at, record.ended_at.unwrap_or_default()));
            }
        }

        RetentionPreview {
            configured_days,
            auto_delete_enabled: false,
            eligible_count,
            eligible_bytes,
            blocked_unarchived_count,
            oldest_eligible_at: oldest_eligible.map(|(_, value)| value),
        }
    }

    /// Generates a small local JPEG thumbnail on demand. Remote-only archive
    /// objects are intentionally not downloaded just to create a preview.
    pub async fn ensure_thumbnail(&self, id: &str) -> Result<PathBuf, String> {
        let record = self
            .get(id)
            .ok_or_else(|| "Recording not found".to_string())?;
        let source = PathBuf::from(&record.local_path);
        if !source.is_file() {
            return Err("Local recording file is unavailable for thumbnail generation".to_string());
        }
        let thumbnail = source.with_extension("jpg");
        if thumbnail.is_file() {
            let source_modified = fs::metadata(&source).and_then(|metadata| metadata.modified());
            let thumbnail_modified =
                fs::metadata(&thumbnail).and_then(|metadata| metadata.modified());
            if let (Ok(source_modified), Ok(thumbnail_modified)) =
                (source_modified, thumbnail_modified)
            {
                if thumbnail_modified >= source_modified {
                    return Ok(thumbnail);
                }
            }
        }

        // Keep a `.jpg` suffix so FFmpeg can infer the image muxer while the
        // incomplete file remains distinguishable from a published thumbnail.
        let temporary = thumbnail.with_extension("part.jpg");
        let source_string = source.to_string_lossy().to_string();
        let temporary_string = temporary.to_string_lossy().to_string();
        let output = Command::new(&self.config.ffmpeg_path)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-ss",
                "0.5",
                "-i",
                &source_string,
                "-frames:v",
                "1",
                "-vf",
                "scale='min(640,iw)':-2",
                "-q:v",
                "5",
                &temporary_string,
            ])
            .output()
            .await
            .map_err(|error| format!("Could not start thumbnail generator: {error}"))?;
        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let _ = tokio::fs::remove_file(&temporary).await;
            return Err(if error.is_empty() {
                format!("FFmpeg thumbnail generation exited with {}", output.status)
            } else {
                error
            });
        }
        tokio::fs::rename(&temporary, &thumbnail)
            .await
            .map_err(|error| format!("Could not publish recording thumbnail: {error}"))?;
        Ok(thumbnail)
    }

    pub async fn reconcile(&self, cameras: &[Camera], tunnels: &TunnelManager) {
        self.reap_finished();
        self.stop_inactive(cameras, tunnels);
        self.index_closed_files(cameras);
        self.backfill_missing_checksums().await;

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
        let mut record = self
            .get(id)
            .ok_or_else(|| "Recording not found".to_string())?;
        if record.status == "archived" && (!self.config.archive_verify || record.archive_verified) {
            return Ok(record);
        }
        if record.status == "archived" && self.config.archive_verify {
            self.verify_archive(&record).await?;
            return self
                .get(id)
                .ok_or_else(|| "Recording disappeared after verification".to_string());
        }
        if record.checksum_sha256.is_none() {
            let checksum = sha256_file_async(Path::new(&record.local_path)).await?;
            self.set_checksum(&record.id, checksum.clone());
            record.checksum_sha256 = Some(checksum);
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

    fn set_checksum(&self, id: &str, checksum: String) {
        let mut changed = false;
        if let Some(record) = self
            .records
            .lock()
            .unwrap()
            .iter_mut()
            .find(|record| record.id == id)
        {
            if record.checksum_sha256.as_deref() != Some(checksum.as_str()) {
                record.checksum_sha256 = Some(checksum);
                changed = true;
            }
        }
        if changed {
            self.persist();
        }
    }

    fn set_archive_verified(&self, id: &str, verified: bool) {
        let mut changed = false;
        if let Some(record) = self
            .records
            .lock()
            .unwrap()
            .iter_mut()
            .find(|record| record.id == id)
        {
            if record.archive_verified != verified {
                record.archive_verified = verified;
                changed = true;
            }
        }
        if changed {
            self.persist();
        }
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
                    checksum_sha256: None,
                    archive_verified: false,
                });
                changed = true;
            }
        }
        drop(records);
        if changed {
            self.persist();
        }
    }

    async fn backfill_missing_checksums(&self) {
        let candidates = self
            .records
            .lock()
            .unwrap()
            .iter()
            .filter(|record| record.checksum_sha256.is_none())
            .filter(|record| Path::new(&record.local_path).is_file())
            .map(|record| (record.id.clone(), record.local_path.clone()))
            .collect::<Vec<_>>();
        let mut changed = false;
        for (id, path) in candidates {
            let Ok(checksum) = sha256_file_async(Path::new(&path)).await else {
                continue;
            };
            if let Some(record) = self
                .records
                .lock()
                .unwrap()
                .iter_mut()
                .find(|record| record.id == id)
            {
                if record.checksum_sha256.is_none() {
                    record.checksum_sha256 = Some(checksum);
                    changed = true;
                }
            }
        }
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

    async fn upload(&self, mut record: Recording) -> Result<(), String> {
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
        if record.checksum_sha256.is_none() {
            let checksum = match sha256_file_async(Path::new(&record.local_path)).await {
                Ok(checksum) => checksum,
                Err(error) => {
                    self.set_status(&record.id, "failed", Some(error.clone()));
                    return Err(error);
                }
            };
            self.set_checksum(&record.id, checksum.clone());
            record.checksum_sha256 = Some(checksum);
        }
        let output = match Command::new("rclone")
            .args([
                "copyto",
                &record.local_path,
                &target,
                "--retries",
                "3",
                "--low-level-retries",
                "10",
                "--retries-sleep",
                "5s",
            ])
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
            if self.config.archive_verify {
                if let Err(error) = self.verify_remote_checksum(&record, &target).await {
                    self.set_archive_verified(&record.id, false);
                    self.set_status(&record.id, "failed", Some(error.clone()));
                    return Err(error);
                }
                self.set_archive_verified(&record.id, true);
            } else {
                self.set_archive_verified(&record.id, false);
            }
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

    async fn verify_archive(&self, record: &Recording) -> Result<(), String> {
        let target = self
            .archive_target(record)
            .ok_or_else(|| "Archive remote is not configured".to_string())?;
        if record.checksum_sha256.is_none() {
            return Err("Local SHA-256 is unavailable for remote verification".to_string());
        }
        match self.verify_remote_checksum(record, &target).await {
            Ok(()) => {
                self.set_archive_verified(&record.id, true);
                Ok(())
            }
            Err(error) => {
                self.set_archive_verified(&record.id, false);
                self.set_status(&record.id, "failed", Some(error.clone()));
                Err(error)
            }
        }
    }

    async fn verify_remote_checksum(&self, record: &Recording, target: &str) -> Result<(), String> {
        let expected = record
            .checksum_sha256
            .as_deref()
            .ok_or_else(|| "Local SHA-256 is unavailable for remote verification".to_string())?;
        let output = Command::new("rclone")
            .args(["hashsum", "SHA-256", target, "--download"])
            .output()
            .await
            .map_err(|error| format!("Could not start rclone verification: {error}"))?;
        if !output.status.success() {
            let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(if message.is_empty() {
                format!("rclone verification exited with {}", output.status)
            } else {
                message
            });
        }
        let actual = parse_hashsum_output(&output.stdout)
            .ok_or_else(|| "rclone verification returned no SHA-256 hash".to_string())?;
        if actual != expected {
            return Err(format!(
                "Remote SHA-256 mismatch for {}: expected {}, got {}",
                record.id, expected, actual
            ));
        }
        Ok(())
    }
}

pub fn archive_poll_interval(config: &AppConfig) -> Duration {
    Duration::from_secs(config.archive_poll_seconds.max(5))
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|error| format!("Could not open file: {error}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| format!("Could not read file: {error}"))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

async fn sha256_file_async(path: &Path) -> Result<String, String> {
    let path = path.to_owned();
    tokio::task::spawn_blocking(move || sha256_file(&path))
        .await
        .map_err(|error| format!("Checksum worker failed: {error}"))?
}

fn parse_hashsum_output(output: &[u8]) -> Option<String> {
    String::from_utf8_lossy(output)
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .find(|value| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .map(str::to_ascii_lowercase)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_file_matches_empty_file_vector() {
        let path = std::env::temp_dir().join(format!("camrelay-sha256-{}.tmp", Uuid::new_v4()));
        fs::write(&path, []).expect("temporary file should be writable");
        let checksum = sha256_file(&path).expect("checksum should be calculated");
        let _ = fs::remove_file(&path);
        assert_eq!(
            checksum,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn parse_hashsum_output_reads_rclone_checksum() {
        let output =
            b"E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855  recording.mp4\n";
        assert_eq!(
            parse_hashsum_output(output).as_deref(),
            Some("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
        );
    }

    #[test]
    fn retention_preview_is_non_destructive_when_disabled() {
        let manager = RecordingManager::new(AppConfig::default(), RtspProxyManager::new());
        let preview = manager.retention_preview();

        assert_eq!(preview.configured_days, 0);
        assert!(!preview.auto_delete_enabled);
        assert_eq!(preview.eligible_count, 0);
        assert_eq!(preview.eligible_bytes, 0);
    }
}
