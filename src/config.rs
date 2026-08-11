use serde::{Deserialize, Serialize};
use std::fs;

fn default_recordings_dir() -> String {
    "recordings".to_string()
}
fn default_recordings_index() -> String {
    "recordings.json".to_string()
}
fn default_segment_seconds() -> u32 {
    300
}
fn default_ffmpeg_path() -> String {
    "ffmpeg".to_string()
}
fn default_archive_root() -> String {
    "camrelay-archive".to_string()
}
fn default_archive_poll_seconds() -> u64 {
    15
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AppConfig {
    pub username: String,
    pub password: String,
    pub web_port: u16,
    #[serde(default)]
    pub recordings_enabled: bool,
    #[serde(default = "default_recordings_dir")]
    pub recordings_dir: String,
    #[serde(default = "default_recordings_index")]
    pub recordings_index: String,
    #[serde(default = "default_segment_seconds")]
    pub segment_seconds: u32,
    #[serde(default = "default_ffmpeg_path")]
    pub ffmpeg_path: String,
    #[serde(default)]
    pub archive_enabled: bool,
    #[serde(default)]
    pub archive_remote: String,
    #[serde(default = "default_archive_root")]
    pub archive_root: String,
    #[serde(default = "default_archive_poll_seconds")]
    pub archive_poll_seconds: u64,
    #[serde(default)]
    pub local_retention_days: u32,
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            username: "admin".to_string(),
            password: "admin".to_string(),
            web_port: 8080,
            recordings_enabled: false,
            recordings_dir: default_recordings_dir(),
            recordings_index: default_recordings_index(),
            segment_seconds: default_segment_seconds(),
            ffmpeg_path: default_ffmpeg_path(),
            archive_enabled: false,
            archive_remote: String::new(),
            archive_root: default_archive_root(),
            archive_poll_seconds: default_archive_poll_seconds(),
            local_retention_days: 0,
        }
    }
}

impl AppConfig {
    pub fn load() -> Self {
        fs::read_to_string("config.json")
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Brand {
    pub id: String,
    pub name: String,
    pub main_server: String,
    pub app_username: String,
    pub app_userkey: String,
}

pub fn load_brands() -> Vec<Brand> {
    fs::read_to_string("brands.json")
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_brands(brands: &[Brand]) -> std::io::Result<()> {
    let content = serde_json::to_string_pretty(brands)?;
    fs::write("brands.json", content)
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Camera {
    pub id: String,
    pub name: String,
    pub brand: String,
    pub serial: String,
    pub username: String,
    pub password: String,
    pub port: u16,
    pub local_port: u16,
    #[serde(default)]
    pub auto_start: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ApiToken {
    pub id: String,
    pub name: String,
    pub token: String,
    pub expires_at: Option<String>,
    pub enabled: bool,
}

pub fn load_tokens() -> Vec<ApiToken> {
    fs::read_to_string("tokens.json")
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_tokens(tokens: &[ApiToken]) -> std::io::Result<()> {
    let content = serde_json::to_string_pretty(tokens)?;
    fs::write("tokens.json", content)
}

pub fn load_cameras() -> Vec<Camera> {
    fs::read_to_string("cameras.json")
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_cameras(cameras: &[Camera]) -> std::io::Result<()> {
    let content = serde_json::to_string_pretty(cameras)?;
    fs::write("cameras.json", content)
}
