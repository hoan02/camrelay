use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AppConfig {
    pub username: String,
    pub password: String,
    pub web_port: u16,
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            username: "admin".to_string(),
            password: "admin".to_string(),
            web_port: 8080,
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
