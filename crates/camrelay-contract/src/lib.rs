//! Stable DTO and error primitives shared by the Camrelay API clients.
//!
//! This crate intentionally contains no Axum, database, or runtime concerns.
//! It is the first seam between the Rust service and the future web/mobile
//! clients; transport adapters can add HTTP-specific behavior around these
//! serializable values.

use serde::{Deserialize, Serialize};

pub const API_VERSION: &str = "v1";

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Owner,
    Admin,
    Operator,
    Viewer,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct FieldError {
    pub field: String,
    pub code: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ApiError {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<FieldError>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

impl ApiError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            fields: Vec::new(),
            request_id: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct CameraSummary {
    pub id: String,
    pub name: String,
    pub brand: String,
    pub serial: String,
    pub local_port: u16,
    pub auto_start: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct RecordingSummary {
    pub id: String,
    pub camera_id: String,
    pub camera_name: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub kind: String,
    pub bytes: u64,
    pub status: String,
    pub archive_available: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ProviderSummary {
    pub id: String,
    pub name: String,
    pub main_server: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ApiTokenSummary {
    pub id: String,
    pub name: String,
    pub expires_at: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct UserSummary {
    pub username: String,
    pub role: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct AuditEventSummary {
    pub id: String,
    pub actor: String,
    pub action: String,
    pub path: String,
    pub status: u16,
    pub created_at: String,
}

impl<T> Page<T> {
    pub fn new(items: Vec<T>, next_cursor: Option<String>) -> Self {
        Self { items, next_cursor }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_error_serializes_without_empty_optional_fields() {
        let value = ApiError::new("auth.invalid_credentials", "Invalid credentials");
        let json = serde_json::to_string(&value).expect("error should serialize");
        assert_eq!(
            json,
            r#"{"code":"auth.invalid_credentials","message":"Invalid credentials"}"#
        );
    }
}
