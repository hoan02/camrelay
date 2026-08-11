//! SQLite persistence foundation for Camrelay.
//!
//! SQLite is the v1 operational source of truth when database mode is enabled.
//! Legacy JSON remains an idempotent import and rollback source so the cutover
//! can be tested without changing the known relay behavior.

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Key, Nonce,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use sqlx::{sqlite::SqlitePoolOptions, Row, SqlitePool};
use std::{fs, path::Path};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("database migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("could not read legacy file: {0}")]
    Io(#[from] std::io::Error),
    #[error("could not parse legacy JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("could not hash password: {0}")]
    PasswordHash(String),
    #[error("CAMRELAY_SECRET_KEY must be a base64-encoded 32-byte key")]
    InvalidSecretKey,
    #[error("CAMRELAY_SECRET_KEY is required to import provider or camera secrets")]
    SecretKeyRequired,
    #[error("could not encrypt secret")]
    SecretEncryption,
}

#[derive(Clone)]
pub struct Storage {
    pool: SqlitePool,
    secret_key: Option<[u8; 32]>,
}

#[derive(Debug, Default, Deserialize)]
pub struct LegacySnapshot {
    pub username: Option<String>,
    pub password: Option<String>,
    #[serde(default)]
    pub brands: Vec<LegacyBrand>,
    #[serde(default)]
    pub cameras: Vec<LegacyCamera>,
    #[serde(default)]
    pub tokens: Vec<LegacyToken>,
}

#[derive(Debug, Deserialize)]
pub struct LegacyBrand {
    pub id: String,
    pub name: String,
    pub main_server: String,
    pub app_username: String,
    pub app_userkey: String,
}

#[derive(Debug, Deserialize)]
pub struct LegacyCamera {
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

#[derive(Debug, Deserialize)]
pub struct LegacyToken {
    pub id: String,
    pub name: String,
    pub token: String,
    pub expires_at: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ImportReport {
    pub users: usize,
    pub providers: usize,
    pub cameras: usize,
    pub tokens: usize,
}

impl Storage {
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let secret_key = match std::env::var("CAMRELAY_SECRET_KEY") {
            Ok(value) => Some(decode_secret_key(&value)?),
            Err(std::env::VarError::NotPresent) => None,
            Err(std::env::VarError::NotUnicode(_)) => return Err(StorageError::InvalidSecretKey),
        };
        Self::open_with_secret_key(path, secret_key).await
    }

    pub async fn open_with_secret_key(
        path: impl AsRef<Path>,
        secret_key: Option<[u8; 32]>,
    ) -> Result<Self, StorageError> {
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(path.as_ref())
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;
        sqlx::migrate!().run(&pool).await?;
        Ok(Self { pool, secret_key })
    }

    pub async fn health_check(&self) -> Result<(), StorageError> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }

    pub async fn camera_count(&self) -> Result<i64, StorageError> {
        let row = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM cameras")
            .fetch_one(&self.pool)
            .await?;
        Ok(row)
    }

    pub async fn list_cameras(&self) -> Result<Vec<StoredCamera>, StorageError> {
        let rows = sqlx::query(
            "SELECT id, name, brand, serial, local_port, auto_start FROM cameras ORDER BY name, id",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(StoredCamera {
                    id: row.try_get("id")?,
                    name: row.try_get("name")?,
                    brand: row.try_get("brand")?,
                    serial: row.try_get("serial")?,
                    local_port: row.try_get::<i64, _>("local_port")? as u16,
                    auto_start: row.try_get::<i64, _>("auto_start")? != 0,
                })
            })
            .collect()
    }

    pub async fn list_providers(&self) -> Result<Vec<StoredProvider>, StorageError> {
        let rows = sqlx::query("SELECT id, name, main_server FROM providers ORDER BY name, id")
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter()
            .map(|row| {
                Ok(StoredProvider {
                    id: row.try_get("id")?,
                    name: row.try_get("name")?,
                    main_server: row.try_get("main_server")?,
                })
            })
            .collect()
    }

    pub async fn verify_user(&self, username: &str, password: &str) -> Result<bool, StorageError> {
        let hash =
            sqlx::query_scalar::<_, String>("SELECT password_hash FROM users WHERE username = ?")
                .bind(username)
                .fetch_optional(&self.pool)
                .await?;
        Ok(hash.is_some_and(|encoded| verify_password(password, &encoded)))
    }

    pub async fn user_role(&self, username: &str) -> Result<Option<String>, StorageError> {
        Ok(
            sqlx::query_scalar::<_, String>("SELECT role FROM users WHERE username = ?")
                .bind(username)
                .fetch_optional(&self.pool)
                .await?,
        )
    }

    pub async fn list_users(&self) -> Result<Vec<StoredUser>, StorageError> {
        let rows = sqlx::query("SELECT username, role, created_at FROM users ORDER BY username")
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter()
            .map(|row| {
                Ok(StoredUser {
                    username: row.try_get("username")?,
                    role: row.try_get("role")?,
                    created_at: row.try_get("created_at")?,
                })
            })
            .collect()
    }

    pub async fn create_user(
        &self,
        username: &str,
        password: &str,
        role: &str,
    ) -> Result<(), StorageError> {
        let password_hash = hash_password(password)?;
        sqlx::query("INSERT INTO users (id, username, password_hash, role) VALUES (?, ?, ?, ?)")
            .bind(format!("user-{}", rand::random::<u128>()))
            .bind(username)
            .bind(password_hash)
            .bind(role)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn create_session(
        &self,
        token: &str,
        username: &str,
        role: &str,
        expires_at: i64,
    ) -> Result<(), StorageError> {
        sqlx::query(
            "INSERT INTO auth_sessions (token_hash, username, role, expires_at) VALUES (?, ?, ?, ?)",
        )
        .bind(hash_session_token(token))
        .bind(username)
        .bind(role)
        .bind(expires_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn find_session(
        &self,
        token: &str,
        now: i64,
    ) -> Result<Option<StoredSession>, StorageError> {
        let row = sqlx::query(
            "SELECT username, role, expires_at FROM auth_sessions WHERE token_hash = ? AND expires_at > ?",
        )
        .bind(hash_session_token(token))
        .bind(now)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|row| {
            Ok(StoredSession {
                username: row.try_get("username")?,
                role: row.try_get("role")?,
                expires_at: row.try_get("expires_at")?,
            })
        })
        .transpose()
    }

    pub async fn delete_session(&self, token: &str) -> Result<(), StorageError> {
        sqlx::query("DELETE FROM auth_sessions WHERE token_hash = ?")
            .bind(hash_session_token(token))
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn rotate_session(
        &self,
        old_token: &str,
        new_token: &str,
        username: &str,
        role: &str,
        expires_at: i64,
    ) -> Result<bool, StorageError> {
        let mut transaction = self.pool.begin().await?;
        let deleted = sqlx::query("DELETE FROM auth_sessions WHERE token_hash = ?")
            .bind(hash_session_token(old_token))
            .execute(&mut *transaction)
            .await?
            .rows_affected();
        if deleted == 0 {
            transaction.rollback().await?;
            return Ok(false);
        }
        sqlx::query(
            "INSERT INTO auth_sessions (token_hash, username, role, expires_at) VALUES (?, ?, ?, ?)",
        )
        .bind(hash_session_token(new_token))
        .bind(username)
        .bind(role)
        .bind(expires_at)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(true)
    }

    pub async fn import_legacy(
        &self,
        snapshot: &LegacySnapshot,
    ) -> Result<ImportReport, StorageError> {
        if (!snapshot.brands.is_empty()
            || !snapshot.cameras.is_empty()
            || !snapshot.tokens.is_empty())
            && self.secret_key.is_none()
        {
            return Err(StorageError::SecretKeyRequired);
        }
        let mut transaction = self.pool.begin().await?;
        let mut report = ImportReport::default();

        if let (Some(username), Some(password)) = (&snapshot.username, &snapshot.password) {
            let password_hash = hash_password(password)?;
            let result = sqlx::query(
                "INSERT INTO users (id, username, password_hash, role) VALUES (?, ?, ?, 'owner') ON CONFLICT(username) DO NOTHING",
            )
            .bind(format!("legacy-user-{username}"))
            .bind(username)
            .bind(password_hash)
            .execute(&mut *transaction)
            .await?;
            report.users = result.rows_affected() as usize;
        }

        for provider in &snapshot.brands {
            let app_username = self.encrypt_secret(&provider.app_username)?;
            let app_userkey = self.encrypt_secret(&provider.app_userkey)?;
            sqlx::query(
                "INSERT INTO providers (id, name, main_server, app_username, app_userkey) VALUES (?, ?, ?, ?, ?) ON CONFLICT(name) DO UPDATE SET main_server = excluded.main_server, app_username = excluded.app_username, app_userkey = excluded.app_userkey",
            )
            .bind(&provider.id)
            .bind(&provider.name)
            .bind(&provider.main_server)
            .bind(app_username)
            .bind(app_userkey)
            .execute(&mut *transaction)
            .await?;
            report.providers += 1;
        }

        for camera in &snapshot.cameras {
            let username = self.encrypt_secret(&camera.username)?;
            let password = self.encrypt_secret(&camera.password)?;
            sqlx::query(
                "INSERT INTO cameras (id, name, brand, serial, username, password, remote_port, local_port, auto_start) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET name = excluded.name, brand = excluded.brand, serial = excluded.serial, username = excluded.username, password = excluded.password, remote_port = excluded.remote_port, local_port = excluded.local_port, auto_start = excluded.auto_start",
            )
            .bind(&camera.id)
            .bind(&camera.name)
            .bind(&camera.brand)
            .bind(&camera.serial)
            .bind(username)
            .bind(password)
            .bind(i64::from(camera.port))
            .bind(i64::from(camera.local_port))
            .bind(i64::from(camera.auto_start as u8))
            .execute(&mut *transaction)
            .await?;
            report.cameras += 1;
        }

        for token in &snapshot.tokens {
            let token_value = self.encrypt_secret(&token.token)?;
            sqlx::query(
                "INSERT INTO api_tokens (id, name, token, expires_at, enabled) VALUES (?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET name = excluded.name, token = excluded.token, expires_at = excluded.expires_at, enabled = excluded.enabled",
            )
            .bind(&token.id)
            .bind(&token.name)
            .bind(token_value)
            .bind(&token.expires_at)
            .bind(i64::from(token.enabled as u8))
            .execute(&mut *transaction)
            .await?;
            report.tokens += 1;
        }

        sqlx::query(
            "INSERT INTO migration_meta (key, value) VALUES ('legacy_json_imported', CURRENT_TIMESTAMP) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(report)
    }

    fn encrypt_secret(&self, value: &str) -> Result<String, StorageError> {
        let Some(secret_key) = self.secret_key else {
            return Err(StorageError::SecretKeyRequired);
        };
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&secret_key));
        let nonce_bytes: [u8; 12] = rand::random();
        let ciphertext = cipher
            .encrypt(Nonce::from_slice(&nonce_bytes), value.as_bytes())
            .map_err(|_| StorageError::SecretEncryption)?;
        let mut encoded = nonce_bytes.to_vec();
        encoded.extend(ciphertext);
        Ok(STANDARD.encode(encoded))
    }

    fn decrypt_secret(&self, value: &str) -> Result<String, StorageError> {
        let Some(secret_key) = self.secret_key else {
            return Err(StorageError::SecretKeyRequired);
        };
        let encoded = STANDARD
            .decode(value.as_bytes())
            .map_err(|_| StorageError::SecretEncryption)?;
        if encoded.len() < 12 {
            return Err(StorageError::SecretEncryption);
        }
        let (nonce_bytes, ciphertext) = encoded.split_at(12);
        let cipher = ChaCha20Poly1305::new(Key::from_slice(&secret_key));
        let plaintext = cipher
            .decrypt(Nonce::from_slice(nonce_bytes), ciphertext)
            .map_err(|_| StorageError::SecretEncryption)?;
        String::from_utf8(plaintext).map_err(|_| StorageError::SecretEncryption)
    }

    pub async fn list_camera_configs(&self) -> Result<Vec<StoredCameraConfig>, StorageError> {
        let rows = sqlx::query(
            "SELECT id, name, brand, serial, username, password, remote_port, local_port, auto_start FROM cameras ORDER BY name, id",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(StoredCameraConfig {
                    id: row.try_get("id")?,
                    name: row.try_get("name")?,
                    brand: row.try_get("brand")?,
                    serial: row.try_get("serial")?,
                    username: self.decrypt_secret(&row.try_get::<String, _>("username")?)?,
                    password: self.decrypt_secret(&row.try_get::<String, _>("password")?)?,
                    port: row.try_get::<i64, _>("remote_port")? as u16,
                    local_port: row.try_get::<i64, _>("local_port")? as u16,
                    auto_start: row.try_get::<i64, _>("auto_start")? != 0,
                })
            })
            .collect()
    }

    pub async fn find_camera_config(
        &self,
        id: &str,
    ) -> Result<Option<StoredCameraConfig>, StorageError> {
        Ok(self
            .list_camera_configs()
            .await?
            .into_iter()
            .find(|camera| camera.id == id))
    }

    pub async fn list_provider_configs(&self) -> Result<Vec<StoredProviderConfig>, StorageError> {
        let rows = sqlx::query(
            "SELECT id, name, main_server, app_username, app_userkey FROM providers ORDER BY name, id",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(StoredProviderConfig {
                    id: row.try_get("id")?,
                    name: row.try_get("name")?,
                    main_server: row.try_get("main_server")?,
                    app_username: self
                        .decrypt_secret(&row.try_get::<String, _>("app_username")?)?,
                    app_userkey: self.decrypt_secret(&row.try_get::<String, _>("app_userkey")?)?,
                })
            })
            .collect()
    }

    pub async fn find_provider_config(
        &self,
        name: &str,
    ) -> Result<Option<StoredProviderConfig>, StorageError> {
        Ok(self
            .list_provider_configs()
            .await?
            .into_iter()
            .find(|provider| provider.name == name))
    }

    pub async fn find_provider_config_by_id(
        &self,
        id: &str,
    ) -> Result<Option<StoredProviderConfig>, StorageError> {
        Ok(self
            .list_provider_configs()
            .await?
            .into_iter()
            .find(|provider| provider.id == id))
    }

    pub async fn upsert_camera(&self, camera: &LegacyCamera) -> Result<(), StorageError> {
        let username = self.encrypt_secret(&camera.username)?;
        let password = self.encrypt_secret(&camera.password)?;
        sqlx::query(
            "INSERT INTO cameras (id, name, brand, serial, username, password, remote_port, local_port, auto_start) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET name = excluded.name, brand = excluded.brand, serial = excluded.serial, username = excluded.username, password = excluded.password, remote_port = excluded.remote_port, local_port = excluded.local_port, auto_start = excluded.auto_start",
        )
        .bind(&camera.id)
        .bind(&camera.name)
        .bind(&camera.brand)
        .bind(&camera.serial)
        .bind(username)
        .bind(password)
        .bind(i64::from(camera.port))
        .bind(i64::from(camera.local_port))
        .bind(i64::from(camera.auto_start as u8))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_camera(&self, id: &str) -> Result<bool, StorageError> {
        let result = sqlx::query("DELETE FROM cameras WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn upsert_provider(&self, provider: &LegacyBrand) -> Result<(), StorageError> {
        let app_username = self.encrypt_secret(&provider.app_username)?;
        let app_userkey = self.encrypt_secret(&provider.app_userkey)?;
        sqlx::query(
            "INSERT INTO providers (id, name, main_server, app_username, app_userkey) VALUES (?, ?, ?, ?, ?) ON CONFLICT(name) DO UPDATE SET id = excluded.id, main_server = excluded.main_server, app_username = excluded.app_username, app_userkey = excluded.app_userkey",
        )
        .bind(&provider.id)
        .bind(&provider.name)
        .bind(&provider.main_server)
        .bind(app_username)
        .bind(app_userkey)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn update_provider(&self, provider: &LegacyBrand) -> Result<bool, StorageError> {
        let app_username = self.encrypt_secret(&provider.app_username)?;
        let app_userkey = self.encrypt_secret(&provider.app_userkey)?;
        let result = sqlx::query(
            "UPDATE providers SET name = ?, main_server = ?, app_username = ?, app_userkey = ? WHERE id = ?",
        )
        .bind(&provider.name)
        .bind(&provider.main_server)
        .bind(app_username)
        .bind(app_userkey)
        .bind(&provider.id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn delete_provider(&self, id: &str) -> Result<bool, StorageError> {
        let result = sqlx::query("DELETE FROM providers WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn list_api_tokens(&self) -> Result<Vec<StoredApiToken>, StorageError> {
        let rows = sqlx::query(
            "SELECT id, name, token, expires_at, enabled FROM api_tokens ORDER BY name, id",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(StoredApiToken {
                    id: row.try_get("id")?,
                    name: row.try_get("name")?,
                    token: self.decrypt_secret(&row.try_get::<String, _>("token")?)?,
                    expires_at: row.try_get("expires_at")?,
                    enabled: row.try_get::<i64, _>("enabled")? != 0,
                })
            })
            .collect()
    }

    pub async fn upsert_api_token(&self, token: &LegacyToken) -> Result<(), StorageError> {
        let token_value = self.encrypt_secret(&token.token)?;
        sqlx::query(
            "INSERT INTO api_tokens (id, name, token, expires_at, enabled) VALUES (?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET name = excluded.name, token = excluded.token, expires_at = excluded.expires_at, enabled = excluded.enabled",
        )
        .bind(&token.id)
        .bind(&token.name)
        .bind(token_value)
        .bind(&token.expires_at)
        .bind(i64::from(token.enabled as u8))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_api_token(&self, id: &str) -> Result<bool, StorageError> {
        let result = sqlx::query("DELETE FROM api_tokens WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub fn load_legacy_snapshot(root: impl AsRef<Path>) -> Result<LegacySnapshot, StorageError> {
        let root = root.as_ref();
        let config: serde_json::Value = read_json(root.join("config.json"))?;
        Ok(LegacySnapshot {
            username: config
                .get("username")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned),
            password: config
                .get("password")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned),
            brands: read_json_if_present(root.join("brands.json"))?,
            cameras: read_json_if_present(root.join("cameras.json"))?,
            tokens: read_json_if_present(root.join("tokens.json"))?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredCamera {
    pub id: String,
    pub name: String,
    pub brand: String,
    pub serial: String,
    pub local_port: u16,
    pub auto_start: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredCameraConfig {
    pub id: String,
    pub name: String,
    pub brand: String,
    pub serial: String,
    pub username: String,
    pub password: String,
    pub port: u16,
    pub local_port: u16,
    pub auto_start: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredProviderConfig {
    pub id: String,
    pub name: String,
    pub main_server: String,
    pub app_username: String,
    pub app_userkey: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredApiToken {
    pub id: String,
    pub name: String,
    pub token: String,
    pub expires_at: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredUser {
    pub username: String,
    pub role: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredSession {
    pub username: String,
    pub role: String,
    pub expires_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredProvider {
    pub id: String,
    pub name: String,
    pub main_server: String,
}

fn read_json<T: for<'de> Deserialize<'de>>(path: impl AsRef<Path>) -> Result<T, StorageError> {
    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}

fn read_json_if_present<T: Default + for<'de> Deserialize<'de>>(
    path: impl AsRef<Path>,
) -> Result<T, StorageError> {
    if path.as_ref().exists() {
        read_json(path)
    } else {
        Ok(T::default())
    }
}

fn decode_secret_key(value: &str) -> Result<[u8; 32], StorageError> {
    let decoded = STANDARD
        .decode(value.as_bytes())
        .map_err(|_| StorageError::InvalidSecretKey)?;
    decoded
        .try_into()
        .map_err(|_| StorageError::InvalidSecretKey)
}

fn hash_session_token(token: &str) -> String {
    STANDARD.encode(Sha256::digest(token.as_bytes()))
}

pub fn hash_password(password: &str) -> Result<String, StorageError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|error| StorageError::PasswordHash(error.to_string()))
}

pub fn verify_password(password: &str, encoded_hash: &str) -> bool {
    let Ok(hash) = PasswordHash::new(encoded_hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &hash)
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn password_hash_round_trip() {
        let encoded = hash_password("correct horse battery staple").expect("hash");
        assert!(verify_password("correct horse battery staple", &encoded));
        assert!(!verify_password("wrong", &encoded));
    }

    #[tokio::test]
    async fn legacy_import_is_idempotent() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("camrelay-storage-{suffix}.sqlite"));
        let storage = Storage::open_with_secret_key(&path, Some([7; 32]))
            .await
            .expect("database should open");
        let snapshot = LegacySnapshot {
            username: Some("admin".to_string()),
            password: Some("change-me".to_string()),
            brands: vec![LegacyBrand {
                id: "provider-1".to_string(),
                name: "Dahua".to_string(),
                main_server: "example.invalid:8800".to_string(),
                app_username: "app-user".to_string(),
                app_userkey: "app-key".to_string(),
            }],
            cameras: vec![LegacyCamera {
                id: "camera-1".to_string(),
                name: "Front door".to_string(),
                brand: "Dahua".to_string(),
                serial: "SERIAL".to_string(),
                username: "admin".to_string(),
                password: "camera-password".to_string(),
                port: 554,
                local_port: 8551,
                auto_start: true,
            }],
            tokens: Vec::new(),
        };

        let first = storage
            .import_legacy(&snapshot)
            .await
            .expect("first import");
        let second = storage
            .import_legacy(&snapshot)
            .await
            .expect("second import");
        storage
            .upsert_camera(&snapshot.cameras[0])
            .await
            .expect("camera upsert");
        storage
            .upsert_provider(&snapshot.brands[0])
            .await
            .expect("provider upsert");
        storage
            .upsert_api_token(&LegacyToken {
                id: "token-1".to_string(),
                name: "Frigate".to_string(),
                token: "camrelay-secret-token".to_string(),
                expires_at: None,
                enabled: true,
            })
            .await
            .expect("token upsert");
        assert_eq!(first.users, 1);
        assert_eq!(second.users, 0);
        assert_eq!(storage.camera_count().await.expect("camera count"), 1);
        let cameras = storage
            .list_camera_configs()
            .await
            .expect("camera secrets should decrypt");
        assert_eq!(cameras[0].password, "camera-password");
        let providers = storage
            .list_provider_configs()
            .await
            .expect("provider secrets should decrypt");
        assert_eq!(providers[0].app_userkey, "app-key");
        assert_eq!(
            storage.list_api_tokens().await.expect("token decrypt")[0].token,
            "camrelay-secret-token"
        );
        assert!(storage
            .verify_user("admin", "change-me")
            .await
            .expect("user verification"));

        drop(storage);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn session_tokens_are_hashed_and_rotated() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("camrelay-session-{suffix}.sqlite"));
        let storage = Storage::open_with_secret_key(&path, None)
            .await
            .expect("database should open");
        storage
            .create_session("session-one", "owner", "owner", 2_000_000_000)
            .await
            .expect("session should persist");
        let stored_hash: String = sqlx::query_scalar("SELECT token_hash FROM auth_sessions")
            .fetch_one(&storage.pool)
            .await
            .expect("hashed token should exist");
        assert_ne!(stored_hash, "session-one");
        assert_eq!(
            storage
                .find_session("session-one", 1_000_000_000)
                .await
                .expect("session lookup")
                .expect("session should be valid")
                .username,
            "owner"
        );
        assert!(storage
            .rotate_session(
                "session-one",
                "session-two",
                "owner",
                "owner",
                2_000_000_000
            )
            .await
            .expect("rotation should succeed"));
        assert!(storage
            .find_session("session-one", 1_000_000_000)
            .await
            .expect("old lookup")
            .is_none());
        assert!(storage
            .find_session("session-two", 1_000_000_000)
            .await
            .expect("new lookup")
            .is_some());

        drop(storage);
        let _ = std::fs::remove_file(path);
    }
}
