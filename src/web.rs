use axum::{
    body::Body,
    extract::{Extension, OriginalUri, Path, Query, Request, State},
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::{get, patch, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    convert::Infallible,
    path::Path as FsPath,
    process::Stdio,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncSeekExt, SeekFrom},
    process::Command,
};
use tokio_util::io::ReaderStream;
use tower_http::services::{ServeDir, ServeFile};
use uuid::Uuid;

use camrelay_contract::{
    ApiTokenSummary, AuditEventSummary, CameraSummary, EventSummary, ProviderSummary,
    RecordingSummary, UserSummary, API_VERSION,
};
use camrelay_storage::{LegacyCamera, Storage, StoredCameraConfig, StoredProviderConfig};

use crate::config::{
    load_brands, load_cameras, load_tokens, save_brands, save_cameras, save_tokens, ApiToken,
    AppConfig, Brand, Camera,
};
use crate::dh::probe_provider;
use crate::live::LiveManager;
use crate::recordings::{Recording, RecordingManager};
use crate::tunnel::{TunnelManager, TunnelStatus};

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub sessions: Arc<Mutex<HashMap<String, AuthPrincipal>>>,
    pub tunnel_manager: Arc<TunnelManager>,
    pub live_manager: LiveManager,
    pub live_tickets: Arc<Mutex<HashMap<String, LiveGrant>>>,
    pub recording_manager: RecordingManager,
    pub playback_tickets: Arc<Mutex<std::collections::HashMap<String, PlaybackGrant>>>,
    pub storage: Option<Storage>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AuthPrincipal {
    pub username: String,
    pub role: String,
    pub auth_type: String,
}

#[derive(Clone)]
pub struct PlaybackGrant {
    pub recording_id: String,
    pub expires_at: Instant,
}

#[derive(Clone)]
pub struct LiveGrant {
    pub camera_id: String,
    pub expires_at: Instant,
}

async fn auth_middleware(State(state): State<AppState>, mut req: Request, next: Next) -> Response {
    let Some(token) = session_token(req.headers()) else {
        return unauthorized_response();
    };
    let Some(principal) = authenticate_token(&state, &token).await else {
        return unauthorized_response();
    };

    let scoped_ticket_request = req.method() == Method::POST
        && (req.uri().path().ends_with("/playback-ticket")
            || req.uri().path().ends_with("/live-ticket"));
    if !matches!(
        req.method(),
        &Method::GET | &Method::HEAD | &Method::OPTIONS
    ) && !is_admin_role(&principal.role)
        && !scoped_ticket_request
    {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "code": "auth.forbidden",
                "message": "This operation requires an owner or admin role."
            })),
        )
            .into_response();
    }

    req.extensions_mut().insert(principal.clone());
    let method = req.method().clone();
    let path = req
        .extensions()
        .get::<OriginalUri>()
        .map(|uri| uri.0.path().to_owned())
        .unwrap_or_else(|| req.uri().path().to_owned());
    let response = next.run(req).await;
    if !matches!(method, Method::GET | Method::HEAD | Method::OPTIONS) {
        if let Some(storage) = &state.storage {
            let _ = storage
                .record_audit(
                    &principal.username,
                    method.as_str(),
                    &path,
                    response.status().as_u16(),
                )
                .await;
        }
    }
    response
}

fn unauthorized_response() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({
            "code": "auth.unauthorized",
            "message": "Authentication is required."
        })),
    )
        .into_response()
}

fn is_admin_role(role: &str) -> bool {
    matches!(role, "owner" | "admin")
}

async fn authenticate_token(state: &AppState, token: &str) -> Option<AuthPrincipal> {
    if let Some(principal) = state.sessions.lock().unwrap().get(token).cloned() {
        return Some(principal);
    }

    if let Some(storage) = &state.storage {
        if let Ok(Some(session)) = storage
            .find_session(token, chrono::Utc::now().timestamp())
            .await
        {
            let principal = AuthPrincipal {
                username: session.username,
                role: session.role,
                auth_type: "session".to_string(),
            };
            state
                .sessions
                .lock()
                .unwrap()
                .insert(token.to_string(), principal.clone());
            return Some(principal);
        }
    }

    let valid = match &state.storage {
        Some(storage) => storage
            .list_api_tokens()
            .await
            .map(|tokens| {
                token_is_valid(
                    token,
                    tokens
                        .into_iter()
                        .map(|token| (token.token, token.enabled, token.expires_at)),
                )
            })
            .unwrap_or(false),
        None => token_is_valid(
            token,
            load_tokens()
                .into_iter()
                .map(|token| (token.token, token.enabled, token.expires_at)),
        ),
    };
    valid.then(|| AuthPrincipal {
        username: "service-token".to_string(),
        role: "admin".to_string(),
        auth_type: "token".to_string(),
    })
}

fn token_is_valid<I>(token: &str, tokens: I) -> bool
where
    I: IntoIterator<Item = (String, bool, Option<String>)>,
{
    let now = chrono::Utc::now();
    tokens.into_iter().any(|(value, enabled, expires_at)| {
        value == token
            && enabled
            && match expires_at {
                None => true,
                Some(exp) => chrono::DateTime::parse_from_rfc3339(&exp)
                    .map(|dt| dt.with_timezone(&chrono::Utc) > now)
                    .unwrap_or(false),
            }
    })
}

/* ─── Auth ─── */

#[derive(Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Serialize)]
struct LoginResponse {
    token: String,
}

async fn login(State(state): State<AppState>, Json(body): Json<LoginRequest>) -> impl IntoResponse {
    let (authenticated, role) = match &state.storage {
        Some(storage) => {
            let authenticated = storage
                .verify_user(&body.username, &body.password)
                .await
                .unwrap_or(false);
            let role = if authenticated {
                storage
                    .user_role(&body.username)
                    .await
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| "owner".to_string())
            } else {
                "owner".to_string()
            };
            (authenticated, role)
        }
        None => (
            body.username == state.config.username && body.password == state.config.password,
            "owner".to_string(),
        ),
    };
    if authenticated {
        let token = Uuid::new_v4().to_string();
        let username = body.username.clone();
        let expires_at = chrono::Utc::now().timestamp() + 86_400;
        if let Some(storage) = &state.storage {
            if let Err(error) = storage
                .create_session(&token, &username, &role, expires_at)
                .await
            {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(serde_json::json!({
                        "code": "auth.session_unavailable",
                        "message": error.to_string()
                    })),
                )
                    .into_response();
            }
        }
        state.sessions.lock().unwrap().insert(
            token.clone(),
            AuthPrincipal {
                username,
                role,
                auth_type: "session".to_string(),
            },
        );
        let cookie =
            format!("camrelay_session={token}; HttpOnly; SameSite=Lax; Path=/; Max-Age=86400");
        (
            StatusCode::OK,
            [(header::SET_COOKIE, cookie)],
            Json(LoginResponse { token }),
        )
            .into_response()
    } else {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "Invalid credentials"})),
        )
            .into_response()
    }
}

async fn logout(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    if let Some(token) = session_token(&headers) {
        state.sessions.lock().unwrap().remove(&token);
        if let Some(storage) = &state.storage {
            let _ = storage.delete_session(&token).await;
        }
    }
    (
        StatusCode::NO_CONTENT,
        [(
            header::SET_COOKIE,
            "camrelay_session=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0",
        )],
    )
        .into_response()
}

async fn refresh_session(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(old_token) = session_token(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Some(principal) = authenticate_token(&state, &old_token).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let token = Uuid::new_v4().to_string();
    let expires_at = chrono::Utc::now().timestamp() + 86_400;
    if let Some(storage) = &state.storage {
        match storage
            .rotate_session(
                &old_token,
                &token,
                &principal.username,
                &principal.role,
                expires_at,
            )
            .await
        {
            Ok(true) => {}
            Ok(false) => return StatusCode::UNAUTHORIZED.into_response(),
            Err(error) => {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(serde_json::json!({
                        "code": "auth.session_unavailable",
                        "message": error.to_string()
                    })),
                )
                    .into_response();
            }
        }
    }
    state.sessions.lock().unwrap().remove(&old_token);
    state
        .sessions
        .lock()
        .unwrap()
        .insert(token.clone(), principal);
    let cookie = format!("camrelay_session={token}; HttpOnly; SameSite=Lax; Path=/; Max-Age=86400");
    (
        StatusCode::OK,
        [(header::SET_COOKIE, cookie)],
        Json(LoginResponse { token }),
    )
        .into_response()
}

async fn current_user(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    let Some(token) = session_token(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    authenticate_token(&state, &token)
        .await
        .map(|principal| Json(principal).into_response())
        .unwrap_or_else(|| StatusCode::UNAUTHORIZED.into_response())
}

fn owner_required(principal: &AuthPrincipal) -> Option<Response> {
    if principal.role == "owner" {
        None
    } else {
        Some(
            (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "code": "users.owner_required",
                    "message": "Only the appliance owner can manage users."
                })),
            )
                .into_response(),
        )
    }
}

async fn get_v1_users(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthPrincipal>,
) -> Response {
    if let Some(response) = owner_required(&principal) {
        return response;
    }
    let Some(storage) = &state.storage else {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "code": "users.sqlite_required",
                "message": "User administration requires SQLite mode."
            })),
        )
            .into_response();
    };
    match storage.list_users().await {
        Ok(users) => Json(
            users
                .into_iter()
                .map(|user| UserSummary {
                    username: user.username,
                    role: user.role,
                    created_at: user.created_at,
                })
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "code": "users.load_failed",
                "message": error.to_string()
            })),
        )
            .into_response(),
    }
}

async fn get_v1_audit(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthPrincipal>,
    Query(query): Query<RecordingsQuery>,
) -> Response {
    if !is_admin_role(&principal.role) {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "code": "audit.admin_required",
                "message": "Only an owner or admin can view audit history."
            })),
        )
            .into_response();
    }
    let Some(storage) = &state.storage else {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "code": "audit.sqlite_required",
                "message": "Audit history requires SQLite mode."
            })),
        )
            .into_response();
    };
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    match storage.list_audit(limit).await {
        Ok(events) => Json(
            events
                .into_iter()
                .map(|event| AuditEventSummary {
                    id: event.id,
                    actor: event.actor,
                    action: event.action,
                    path: event.path,
                    status: event.status,
                    created_at: event.created_at,
                })
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "code": "audit.load_failed",
                "message": error.to_string()
            })),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
struct UserPayload {
    username: String,
    password: String,
    role: String,
}

fn user_role_is_valid(role: &str) -> bool {
    matches!(role, "admin" | "operator" | "viewer")
}

async fn create_v1_user(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthPrincipal>,
    Json(body): Json<UserPayload>,
) -> Response {
    if let Some(response) = owner_required(&principal) {
        return response;
    }
    let username = body.username.trim();
    if username.is_empty()
        || username.len() > 128
        || body.password.len() < 8
        || !user_role_is_valid(&body.role)
    {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({
                "code": "users.invalid_input",
                "message": "Username, a password of at least 8 characters, and a valid non-owner role are required."
            })),
        )
            .into_response();
    }
    let Some(storage) = &state.storage else {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "code": "users.sqlite_required",
                "message": "User administration requires SQLite mode."
            })),
        )
            .into_response();
    };
    if let Err(error) = storage
        .create_user(username, &body.password, &body.role)
        .await
    {
        let status = if error.to_string().contains("UNIQUE") {
            StatusCode::CONFLICT
        } else {
            StatusCode::INTERNAL_SERVER_ERROR
        };
        return (
            status,
            Json(serde_json::json!({
                "code": "users.create_failed",
                "message": error.to_string()
            })),
        )
            .into_response();
    }
    match storage.list_users().await {
        Ok(users) => users
            .into_iter()
            .find(|user| user.username == username)
            .map(|user| {
                (
                    StatusCode::CREATED,
                    Json(UserSummary {
                        username: user.username,
                        role: user.role,
                        created_at: user.created_at,
                    }),
                )
                    .into_response()
            })
            .unwrap_or_else(|| StatusCode::INTERNAL_SERVER_ERROR.into_response()),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "code": "users.load_failed",
                "message": error.to_string()
            })),
        )
            .into_response(),
    }
}

fn session_token(headers: &HeaderMap) -> Option<String> {
    if let Some(token) = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
    {
        return Some(token.to_string());
    }

    headers
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            value.split(';').find_map(|pair| {
                let (name, value) = pair.trim().split_once('=')?;
                (name == "camrelay_session").then(|| value.to_string())
            })
        })
}

async fn find_camera_config(state: &AppState, id: &str) -> Result<Option<Camera>, String> {
    match &state.storage {
        Some(storage) => storage
            .find_camera_config(id)
            .await
            .map(|camera| camera.map(camera_from_storage))
            .map_err(|error| error.to_string()),
        None => Ok(load_cameras().into_iter().find(|camera| camera.id == id)),
    }
}

async fn v1_health(State(state): State<AppState>) -> impl IntoResponse {
    let (storage, storage_ok, camera_count) = match &state.storage {
        Some(storage) => match storage.health_check().await {
            Ok(()) => (
                "sqlite",
                true,
                storage.camera_count().await.unwrap_or_default(),
            ),
            Err(_) => ("sqlite", false, 0),
        },
        None => ("legacy_json", true, load_cameras().len() as i64),
    };
    let status = if storage_ok { "ok" } else { "degraded" };
    Json(serde_json::json!({
        "api_version": API_VERSION,
        "status": status,
        "storage": storage,
        "camera_count": camera_count,
    }))
}

async fn v1_readiness(State(state): State<AppState>) -> impl IntoResponse {
    let checks = if let Some(storage) = &state.storage {
        serde_json::json!({
            "storage": storage.health_check().await.is_ok(),
            "mode": "sqlite"
        })
    } else {
        serde_json::json!({
            "storage": true,
            "mode": "legacy_json"
        })
    };
    let ready = checks
        .get("storage")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let status = if ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (
        status,
        Json(serde_json::json!({
            "api_version": API_VERSION,
            "status": if ready { "ready" } else { "not_ready" },
            "checks": checks
        })),
    )
}

async fn system_stream(
    State(state): State<AppState>,
) -> Sse<impl futures_core::Stream<Item = Result<Event, Infallible>>> {
    let stream = async_stream::stream! {
        loop {
            let (storage, storage_ok, camera_count) = match &state.storage {
                Some(storage) => match storage.health_check().await {
                    Ok(()) => ("sqlite", true, storage.camera_count().await.unwrap_or_default()),
                    Err(_) => ("sqlite", false, 0),
                },
                None => ("legacy_json", true, load_cameras().len() as i64),
            };
            let camera_ids: Vec<String> = match &state.storage {
                Some(storage) => storage
                    .list_cameras()
                    .await
                    .map(|cameras| cameras.into_iter().map(|camera| camera.id).collect())
                    .unwrap_or_default(),
                None => load_cameras().into_iter().map(|camera| camera.id).collect(),
            };
            let tunnels = camera_ids
                .into_iter()
                .map(|camera_id| {
                    let status = match state.tunnel_manager.status(&camera_id) {
                        TunnelStatus::Running => "running",
                        TunnelStatus::Starting => "starting",
                        TunnelStatus::Error(_) => "error",
                        TunnelStatus::Stopped => "stopped",
                    };
                    serde_json::json!({ "id": camera_id, "status": status })
                })
                .collect::<Vec<_>>();
            let payload = serde_json::json!({
                "api_version": API_VERSION,
                "status": if storage_ok { "ok" } else { "degraded" },
                "storage": storage,
                "camera_count": camera_count,
                "tunnels": tunnels,
            });
            yield Ok(Event::default().event("system").json_data(payload).unwrap_or_else(|_| Event::default()));
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    };
    Sse::new(stream).keep_alive(KeepAlive::default())
}

async fn get_v1_cameras(State(state): State<AppState>) -> impl IntoResponse {
    if let Some(storage) = &state.storage {
        return match storage.list_cameras().await {
            Ok(cameras) => Json(
                cameras
                    .into_iter()
                    .map(|camera| CameraSummary {
                        id: camera.id,
                        name: camera.name,
                        brand: camera.brand,
                        serial: camera.serial,
                        local_port: camera.local_port,
                        auto_start: camera.auto_start,
                    })
                    .collect::<Vec<_>>(),
            )
            .into_response(),
            Err(error) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": error.to_string()})),
            )
                .into_response(),
        };
    }

    Json(
        load_cameras()
            .into_iter()
            .map(|camera| CameraSummary {
                id: camera.id,
                name: camera.name,
                brand: camera.brand,
                serial: camera.serial,
                local_port: camera.local_port,
                auto_start: camera.auto_start,
            })
            .collect::<Vec<_>>(),
    )
    .into_response()
}

async fn get_v1_camera(State(state): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    if let Some(storage) = &state.storage {
        return match storage.find_camera_config(&id).await {
            Ok(Some(camera)) => Json(CameraSummary {
                id: camera.id,
                name: camera.name,
                brand: camera.brand,
                serial: camera.serial,
                local_port: camera.local_port,
                auto_start: camera.auto_start,
            })
            .into_response(),
            Ok(None) => StatusCode::NOT_FOUND.into_response(),
            Err(error) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    serde_json::json!({"code": "camera.load_failed", "message": error.to_string()}),
                ),
            )
                .into_response(),
        };
    }

    match load_cameras().into_iter().find(|camera| camera.id == id) {
        Some(camera) => Json(CameraSummary {
            id: camera.id,
            name: camera.name,
            brand: camera.brand,
            serial: camera.serial,
            local_port: camera.local_port,
            auto_start: camera.auto_start,
        })
        .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

#[derive(Serialize)]
struct CameraDiagnosticsResponse {
    camera_id: String,
    provider: String,
    provider_configured: bool,
    tunnel_status: String,
    tunnel_error: Option<String>,
    local_port: u16,
    rtsp_path: &'static str,
    next_action: String,
}

async fn get_v1_camera_diagnostics(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let camera = match &state.storage {
        Some(storage) => match storage.list_cameras().await {
            Ok(cameras) => cameras
                .into_iter()
                .find(|camera| camera.id == id)
                .map(|camera| CameraSummary {
                    id: camera.id,
                    name: camera.name,
                    brand: camera.brand,
                    serial: camera.serial,
                    local_port: camera.local_port,
                    auto_start: camera.auto_start,
                }),
            Err(error) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({
                        "code": "camera.diagnostics_unavailable",
                        "message": error.to_string()
                    })),
                )
                    .into_response();
            }
        },
        None => load_cameras()
            .into_iter()
            .find(|camera| camera.id == id)
            .map(|camera| CameraSummary {
                id: camera.id,
                name: camera.name,
                brand: camera.brand,
                serial: camera.serial,
                local_port: camera.local_port,
                auto_start: camera.auto_start,
            }),
    };
    let Some(camera) = camera else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let provider_configured = match &state.storage {
        Some(storage) => storage
            .list_providers()
            .await
            .map(|providers| {
                providers
                    .iter()
                    .any(|provider| provider.name == camera.brand)
            })
            .unwrap_or(false),
        None => load_brands()
            .iter()
            .any(|provider| provider.name == camera.brand),
    };
    let (tunnel_status, tunnel_error) = match state.tunnel_manager.status(&id) {
        TunnelStatus::Running => ("running".to_string(), None),
        TunnelStatus::Starting => ("starting".to_string(), None),
        TunnelStatus::Stopped => ("stopped".to_string(), None),
        TunnelStatus::Error(error) => ("error".to_string(), Some(error)),
    };
    let next_action = if !provider_configured {
        "Configure the provider profile before starting the relay.".to_string()
    } else if tunnel_status == "running" {
        "The local RTSP endpoint is ready for FFmpeg, Frigate, or a media gateway.".to_string()
    } else {
        "Start the relay, then verify the local RTSP endpoint with ffplay.".to_string()
    };

    Json(CameraDiagnosticsResponse {
        camera_id: camera.id,
        provider: camera.brand,
        provider_configured,
        tunnel_status,
        tunnel_error,
        local_port: camera.local_port,
        rtsp_path: "/cam/realmonitor?channel=1&subtype=0",
        next_action,
    })
    .into_response()
}

#[derive(Deserialize, Default)]
struct CameraUpdatePayload {
    name: Option<String>,
    brand: Option<String>,
    serial: Option<String>,
    username: Option<String>,
    password: Option<String>,
    port: Option<u16>,
    local_port: Option<u16>,
    auto_start: Option<bool>,
}

fn camera_is_valid(camera: &Camera) -> bool {
    !camera.name.trim().is_empty()
        && !camera.brand.trim().is_empty()
        && !camera.serial.trim().is_empty()
        && !camera.username.trim().is_empty()
        && !camera.password.is_empty()
        && camera.port > 0
        && camera.local_port > 0
}

async fn update_v1_camera(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<CameraUpdatePayload>,
) -> impl IntoResponse {
    let current = match &state.storage {
        Some(storage) => match storage.find_camera_config(&id).await {
            Ok(Some(camera)) => Some(camera_from_storage(camera)),
            Ok(None) => None,
            Err(error) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"code": "camera.load_failed", "message": error.to_string()})),
                )
                    .into_response();
            }
        },
        None => load_cameras().into_iter().find(|camera| camera.id == id),
    };
    let Some(mut camera) = current else {
        return StatusCode::NOT_FOUND.into_response();
    };

    if let Some(value) = body.name {
        camera.name = value;
    }
    if let Some(value) = body.brand {
        camera.brand = value;
    }
    if let Some(value) = body.serial {
        camera.serial = value;
    }
    if let Some(value) = body.username {
        camera.username = value;
    }
    if let Some(value) = body.password {
        camera.password = value;
    }
    if let Some(value) = body.port {
        camera.port = value;
    }
    if let Some(value) = body.local_port {
        camera.local_port = value;
    }
    if let Some(value) = body.auto_start {
        camera.auto_start = value;
    }
    if !camera_is_valid(&camera) {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({
                "code": "camera.invalid_input",
                "message": "The resulting camera configuration is incomplete or invalid."
            })),
        )
            .into_response();
    }

    let result = if let Some(storage) = &state.storage {
        storage
            .upsert_camera(&LegacyCamera {
                id: camera.id.clone(),
                name: camera.name.clone(),
                brand: camera.brand.clone(),
                serial: camera.serial.clone(),
                username: camera.username.clone(),
                password: camera.password.clone(),
                port: camera.port,
                local_port: camera.local_port,
                auto_start: camera.auto_start,
            })
            .await
            .map_err(|error| error.to_string())
    } else {
        let mut cameras = load_cameras();
        if let Some(existing) = cameras.iter_mut().find(|existing| existing.id == id) {
            *existing = camera.clone();
            save_cameras(&cameras).map_err(|error| error.to_string())
        } else {
            Err("Camera not found".to_string())
        }
    };
    if let Err(error) = result {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"code": "camera.save_failed", "message": error})),
        )
            .into_response();
    }
    Json(CameraSummary {
        id: camera.id,
        name: camera.name,
        brand: camera.brand,
        serial: camera.serial,
        local_port: camera.local_port,
        auto_start: camera.auto_start,
    })
    .into_response()
}

async fn delete_v1_camera(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    state.tunnel_manager.stop(&id);
    state.live_manager.stop(&id);
    let result = if let Some(storage) = &state.storage {
        storage.delete_camera(&id).await
    } else {
        let mut cameras = load_cameras();
        let before = cameras.len();
        cameras.retain(|camera| camera.id != id);
        if cameras.len() == before {
            Ok(false)
        } else {
            save_cameras(&cameras)
                .map(|_| true)
                .map_err(camrelay_storage::StorageError::from)
        }
    };
    match result {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"code": "camera.delete_failed", "message": error.to_string()})),
        )
            .into_response(),
    }
}

async fn create_v1_camera(
    State(state): State<AppState>,
    Json(body): Json<CameraPayload>,
) -> impl IntoResponse {
    if body.name.trim().is_empty()
        || body.brand.trim().is_empty()
        || body.serial.trim().is_empty()
        || body.username.trim().is_empty()
        || body.password.is_empty()
        || body.port == 0
        || body.local_port == 0
    {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({
                "code": "camera.invalid_input",
                "message": "Name, provider, serial, credentials, and valid ports are required."
            })),
        )
            .into_response();
    }

    let camera = Camera {
        id: Uuid::new_v4().to_string(),
        name: body.name,
        brand: body.brand,
        serial: body.serial,
        username: body.username,
        password: body.password,
        port: body.port,
        local_port: body.local_port,
        auto_start: body.auto_start,
    };
    if let Some(storage) = &state.storage {
        let legacy = LegacyCamera {
            id: camera.id.clone(),
            name: camera.name.clone(),
            brand: camera.brand.clone(),
            serial: camera.serial.clone(),
            username: camera.username.clone(),
            password: camera.password.clone(),
            port: camera.port,
            local_port: camera.local_port,
            auto_start: camera.auto_start,
        };
        if let Err(error) = storage.upsert_camera(&legacy).await {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "code": "camera.save_failed",
                    "message": error.to_string()
                })),
            )
                .into_response();
        }
    } else {
        let mut cameras = load_cameras();
        cameras.push(camera.clone());
        if save_cameras(&cameras).is_err() {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "code": "camera.save_failed",
                    "message": "Could not save camera configuration."
                })),
            )
                .into_response();
        }
    }
    (
        StatusCode::CREATED,
        Json(CameraSummary {
            id: camera.id,
            name: camera.name,
            brand: camera.brand,
            serial: camera.serial,
            local_port: camera.local_port,
            auto_start: camera.auto_start,
        }),
    )
        .into_response()
}

/* ─── Brand handlers ─── */

async fn get_v1_recordings(
    State(state): State<AppState>,
    Query(query): Query<RecordingsQuery>,
) -> impl IntoResponse {
    Json(
        state
            .recording_manager
            .list_filtered(
                query.camera_id.as_deref(),
                query.status.as_deref(),
                query.limit.unwrap_or(100),
            )
            .into_iter()
            .map(|recording| {
                let archive_available = recording.status == "archived";
                RecordingSummary {
                    id: recording.id,
                    camera_id: recording.camera_id,
                    camera_name: recording.camera_name,
                    started_at: recording.started_at,
                    ended_at: recording.ended_at,
                    kind: recording.kind,
                    bytes: recording.bytes,
                    status: recording.status,
                    archive_available,
                    checksum_sha256: recording.checksum_sha256,
                    archive_verified: recording.archive_verified,
                }
            })
            .collect::<Vec<_>>(),
    )
}

async fn get_v1_events(
    State(state): State<AppState>,
    Query(query): Query<RecordingsQuery>,
) -> impl IntoResponse {
    // Until a durable event table exists, closed recording segments are the
    // only event source with a stable persisted timestamp. The `source` field
    // keeps this boundary honest for clients that will later consume motion
    // and system events as well.
    let events = state
        .recording_manager
        .list_filtered(
            query.camera_id.as_deref(),
            query.status.as_deref(),
            query.limit.unwrap_or(100),
        )
        .into_iter()
        .map(|recording| EventSummary {
            id: format!("recording:{}", recording.id),
            kind: "recording.segment".to_string(),
            recording_id: Some(recording.id.clone()),
            camera_id: recording.camera_id,
            camera_name: recording.camera_name,
            occurred_at: recording
                .ended_at
                .unwrap_or_else(|| recording.started_at.clone()),
            severity: "info".to_string(),
            message: format!(
                "{} recording segment is {}",
                recording.kind, recording.status
            ),
            source: "recording_index".to_string(),
        })
        .collect::<Vec<_>>();
    Json(events).into_response()
}

fn recording_summary(recording: Recording) -> RecordingSummary {
    let archive_available = recording.status == "archived";
    RecordingSummary {
        id: recording.id,
        camera_id: recording.camera_id,
        camera_name: recording.camera_name,
        started_at: recording.started_at,
        ended_at: recording.ended_at,
        kind: recording.kind,
        bytes: recording.bytes,
        status: recording.status,
        archive_available,
        checksum_sha256: recording.checksum_sha256,
        archive_verified: recording.archive_verified,
    }
}

async fn get_v1_recording(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.recording_manager.get(&id) {
        Some(recording) => Json(recording_summary(recording)).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn archive_v1_recording(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.recording_manager.archive(&id).await {
        Ok(recording) => Json(recording_summary(recording)).into_response(),
        Err(error) => (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({"code": "recording.archive_failed", "message": error})),
        )
            .into_response(),
    }
}

async fn get_v1_tokens(State(state): State<AppState>) -> impl IntoResponse {
    if let Some(storage) = &state.storage {
        return match storage.list_api_tokens().await {
            Ok(tokens) => Json(
                tokens
                    .into_iter()
                    .map(|token| ApiTokenSummary {
                        id: token.id,
                        name: token.name,
                        expires_at: token.expires_at,
                        enabled: token.enabled,
                    })
                    .collect::<Vec<_>>(),
            )
            .into_response(),
            Err(error) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": error.to_string()})),
            )
                .into_response(),
        };
    }

    Json(
        load_tokens()
            .into_iter()
            .map(|token| ApiTokenSummary {
                id: token.id,
                name: token.name,
                expires_at: token.expires_at,
                enabled: token.enabled,
            })
            .collect::<Vec<_>>(),
    )
    .into_response()
}

async fn create_v1_token(
    State(state): State<AppState>,
    Json(body): Json<TokenPayload>,
) -> impl IntoResponse {
    if body.name.trim().is_empty() {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({
                "code": "token.invalid_input",
                "message": "Token name is required."
            })),
        )
            .into_response();
    }
    let token = ApiToken {
        id: Uuid::new_v4().to_string(),
        name: body.name,
        token: format!("camrelay_{}", Uuid::new_v4().to_string().replace('-', "")),
        expires_at: body.expires_at,
        enabled: body.enabled,
    };
    let result = if let Some(storage) = &state.storage {
        storage
            .upsert_api_token(&camrelay_storage::LegacyToken {
                id: token.id.clone(),
                name: token.name.clone(),
                token: token.token.clone(),
                expires_at: token.expires_at.clone(),
                enabled: token.enabled,
            })
            .await
            .map_err(|error| error.to_string())
    } else {
        let mut tokens = load_tokens();
        tokens.push(token.clone());
        save_tokens(&tokens).map_err(|error| error.to_string())
    };
    if let Err(error) = result {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"code": "token.save_failed", "message": error})),
        )
            .into_response();
    }
    (
        StatusCode::CREATED,
        Json(serde_json::json!({
            "id": token.id,
            "name": token.name,
            "expires_at": token.expires_at,
            "enabled": token.enabled,
            "token": token.token
        })),
    )
        .into_response()
}

#[derive(Deserialize, Default)]
struct TokenPatchPayload {
    name: Option<String>,
    expires_at: Option<String>,
    enabled: Option<bool>,
}

async fn update_v1_token(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<TokenPatchPayload>,
) -> impl IntoResponse {
    let current = match &state.storage {
        Some(storage) => match storage.list_api_tokens().await {
            Ok(tokens) => tokens
                .into_iter()
                .find(|token| token.id == id)
                .map(|token| ApiToken {
                    id: token.id,
                    name: token.name,
                    token: token.token,
                    expires_at: token.expires_at,
                    enabled: token.enabled,
                }),
            Err(error) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"code": "token.load_failed", "message": error.to_string()})),
                )
                    .into_response();
            }
        },
        None => load_tokens().into_iter().find(|token| token.id == id),
    };
    let Some(mut token) = current else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if let Some(name) = body.name {
        token.name = name;
    }
    if let Some(expires_at) = body.expires_at {
        token.expires_at = Some(expires_at);
    }
    if let Some(enabled) = body.enabled {
        token.enabled = enabled;
    }
    if token.name.trim().is_empty() {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({"code": "token.invalid_input", "message": "Token name is required."})),
        )
            .into_response();
    }
    let result = if let Some(storage) = &state.storage {
        storage
            .upsert_api_token(&camrelay_storage::LegacyToken {
                id: token.id.clone(),
                name: token.name.clone(),
                token: token.token.clone(),
                expires_at: token.expires_at.clone(),
                enabled: token.enabled,
            })
            .await
            .map_err(|error| error.to_string())
    } else {
        let mut tokens = load_tokens();
        if let Some(existing) = tokens.iter_mut().find(|existing| existing.id == id) {
            *existing = token.clone();
            save_tokens(&tokens).map_err(|error| error.to_string())
        } else {
            Err("Token not found".to_string())
        }
    };
    if let Err(error) = result {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"code": "token.save_failed", "message": error})),
        )
            .into_response();
    }
    Json(ApiTokenSummary {
        id: token.id,
        name: token.name,
        expires_at: token.expires_at,
        enabled: token.enabled,
    })
    .into_response()
}

async fn delete_v1_token(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let result = if let Some(storage) = &state.storage {
        storage.delete_api_token(&id).await
    } else {
        let mut tokens = load_tokens();
        let before = tokens.len();
        tokens.retain(|token| token.id != id);
        if tokens.len() == before {
            Ok(false)
        } else {
            save_tokens(&tokens)
                .map(|_| true)
                .map_err(camrelay_storage::StorageError::from)
        }
    };
    match result {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"code": "token.delete_failed", "message": error.to_string()})),
        )
            .into_response(),
    }
}

async fn get_v1_providers(State(state): State<AppState>) -> impl IntoResponse {
    if let Some(storage) = &state.storage {
        return match storage.list_providers().await {
            Ok(providers) => Json(
                providers
                    .into_iter()
                    .map(|provider| ProviderSummary {
                        id: provider.id,
                        name: provider.name,
                        main_server: provider.main_server,
                    })
                    .collect::<Vec<_>>(),
            )
            .into_response(),
            Err(error) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": error.to_string()})),
            )
                .into_response(),
        };
    }

    Json(
        load_brands()
            .into_iter()
            .map(|brand| ProviderSummary {
                id: brand.id,
                name: brand.name,
                main_server: brand.main_server,
            })
            .collect::<Vec<_>>(),
    )
    .into_response()
}

async fn get_v1_provider(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Some(storage) = &state.storage {
        return match storage.find_provider_config_by_id(&id).await {
            Ok(Some(provider)) => Json(ProviderSummary {
                id: provider.id,
                name: provider.name,
                main_server: provider.main_server,
            })
            .into_response(),
            Ok(None) => StatusCode::NOT_FOUND.into_response(),
            Err(error) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"code": "provider.load_failed", "message": error.to_string()})),
            )
                .into_response(),
        };
    }

    match load_brands().into_iter().find(|provider| provider.id == id) {
        Some(provider) => Json(ProviderSummary {
            id: provider.id,
            name: provider.name,
            main_server: provider.main_server,
        })
        .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

#[derive(Deserialize, Default)]
struct ProviderUpdatePayload {
    name: Option<String>,
    main_server: Option<String>,
    app_username: Option<String>,
    app_userkey: Option<String>,
}

async fn update_v1_provider(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<ProviderUpdatePayload>,
) -> impl IntoResponse {
    let current = match &state.storage {
        Some(storage) => match storage.find_provider_config_by_id(&id).await {
            Ok(Some(provider)) => Some(Brand {
                id: provider.id,
                name: provider.name,
                main_server: provider.main_server,
                app_username: provider.app_username,
                app_userkey: provider.app_userkey,
            }),
            Ok(None) => None,
            Err(error) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"code": "provider.load_failed", "message": error.to_string()})),
                )
                    .into_response();
            }
        },
        None => load_brands().into_iter().find(|provider| provider.id == id),
    };
    let Some(mut provider) = current else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if let Some(value) = body.name {
        if value != provider.name {
            return (
                StatusCode::CONFLICT,
                Json(serde_json::json!({
                    "code": "provider.name_immutable",
                    "message": "Provider names are identifiers for camera compatibility and cannot be changed."
                })),
            )
                .into_response();
        }
    }
    if let Some(value) = body.main_server {
        provider.main_server = value;
    }
    if let Some(value) = body.app_username {
        provider.app_username = value;
    }
    if let Some(value) = body.app_userkey {
        provider.app_userkey = value;
    }
    if provider.main_server.trim().is_empty()
        || provider.app_username.trim().is_empty()
        || provider.app_userkey.trim().is_empty()
    {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({
                "code": "provider.invalid_input",
                "message": "Signaling server, app username, and app userkey are required."
            })),
        )
            .into_response();
    }

    let result = if let Some(storage) = &state.storage {
        storage
            .update_provider(&camrelay_storage::LegacyBrand {
                id: provider.id.clone(),
                name: provider.name.clone(),
                main_server: provider.main_server.clone(),
                app_username: provider.app_username.clone(),
                app_userkey: provider.app_userkey.clone(),
            })
            .await
            .map(|found| {
                if found {
                    Ok(())
                } else {
                    Err("Provider not found".to_string())
                }
            })
            .unwrap_or_else(|error| Err(error.to_string()))
    } else {
        let mut providers = load_brands();
        if let Some(existing) = providers.iter_mut().find(|existing| existing.id == id) {
            *existing = provider.clone();
            save_brands(&providers).map_err(|error| error.to_string())
        } else {
            Err("Provider not found".to_string())
        }
    };
    if let Err(error) = result {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"code": "provider.save_failed", "message": error})),
        )
            .into_response();
    }
    Json(ProviderSummary {
        id: provider.id,
        name: provider.name,
        main_server: provider.main_server,
    })
    .into_response()
}

async fn delete_v1_provider(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let provider = match &state.storage {
        Some(storage) => storage
            .find_provider_config_by_id(&id)
            .await
            .ok()
            .flatten()
            .map(|provider| provider.name),
        None => load_brands()
            .into_iter()
            .find(|provider| provider.id == id)
            .map(|provider| provider.name),
    };
    let Some(provider_name) = provider else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let used = match &state.storage {
        Some(storage) => storage
            .list_cameras()
            .await
            .map(|cameras| {
                cameras
                    .into_iter()
                    .any(|camera| camera.brand == provider_name)
            })
            .unwrap_or(false),
        None => load_cameras()
            .into_iter()
            .any(|camera| camera.brand == provider_name),
    };
    if used {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "code": "provider.in_use",
                "message": "The provider is still assigned to one or more cameras."
            })),
        )
            .into_response();
    }
    let result = if let Some(storage) = &state.storage {
        storage.delete_provider(&id).await
    } else {
        let mut providers = load_brands();
        let before = providers.len();
        providers.retain(|provider| provider.id != id);
        if providers.len() == before {
            Ok(false)
        } else {
            save_brands(&providers)
                .map(|_| true)
                .map_err(camrelay_storage::StorageError::from)
        }
    };
    match result {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                serde_json::json!({"code": "provider.delete_failed", "message": error.to_string()}),
            ),
        )
            .into_response(),
    }
}

async fn create_v1_provider(
    State(state): State<AppState>,
    Json(body): Json<BrandPayload>,
) -> impl IntoResponse {
    if body.name.trim().is_empty()
        || body.main_server.trim().is_empty()
        || body.app_username.trim().is_empty()
        || body.app_userkey.trim().is_empty()
    {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({
                "code": "provider.invalid_input",
                "message": "Name, signaling server, app username, and app userkey are required."
            })),
        )
            .into_response();
    }

    let provider = Brand {
        id: Uuid::new_v4().to_string(),
        name: body.name,
        main_server: body.main_server,
        app_username: body.app_username,
        app_userkey: body.app_userkey,
    };
    if let Some(storage) = &state.storage {
        let legacy = camrelay_storage::LegacyBrand {
            id: provider.id.clone(),
            name: provider.name.clone(),
            main_server: provider.main_server.clone(),
            app_username: provider.app_username.clone(),
            app_userkey: provider.app_userkey.clone(),
        };
        if let Err(error) = storage.upsert_provider(&legacy).await {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "code": "provider.save_failed",
                    "message": error.to_string()
                })),
            )
                .into_response();
        }
    } else {
        let mut providers = load_brands();
        providers.push(provider.clone());
        if save_brands(&providers).is_err() {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "code": "provider.save_failed",
                    "message": "Could not save provider configuration."
                })),
            )
                .into_response();
        }
    }
    (
        StatusCode::CREATED,
        Json(ProviderSummary {
            id: provider.id,
            name: provider.name,
            main_server: provider.main_server,
        }),
    )
        .into_response()
}

#[derive(Serialize)]
struct TunnelSummary {
    id: String,
    status: String,
}

async fn get_v1_tunnels(State(state): State<AppState>) -> impl IntoResponse {
    let camera_ids: Vec<String> = match &state.storage {
        Some(storage) => storage
            .list_cameras()
            .await
            .map(|cameras| cameras.into_iter().map(|camera| camera.id).collect())
            .unwrap_or_default(),
        None => load_cameras().into_iter().map(|camera| camera.id).collect(),
    };
    Json(
        camera_ids
            .into_iter()
            .map(|camera| TunnelSummary {
                id: camera.clone(),
                status: match state.tunnel_manager.status(&camera) {
                    TunnelStatus::Running => "running".to_string(),
                    TunnelStatus::Starting => "starting".to_string(),
                    TunnelStatus::Error(error) => format!("error: {error}"),
                    TunnelStatus::Stopped => "stopped".to_string(),
                },
            })
            .collect::<Vec<_>>(),
    )
}

async fn get_brands_handler() -> impl IntoResponse {
    Json(load_brands())
}

#[derive(Deserialize)]
struct BrandPayload {
    name: String,
    main_server: String,
    app_username: String,
    app_userkey: String,
}

async fn create_brand_handler(Json(body): Json<BrandPayload>) -> impl IntoResponse {
    let mut brands = load_brands();
    let brand = Brand {
        id: Uuid::new_v4().to_string(),
        name: body.name,
        main_server: body.main_server,
        app_username: body.app_username,
        app_userkey: body.app_userkey,
    };
    brands.push(brand.clone());
    let _ = save_brands(&brands);
    (StatusCode::CREATED, Json(brand))
}

async fn update_brand_handler(
    Path(id): Path<String>,
    Json(body): Json<BrandPayload>,
) -> impl IntoResponse {
    let mut brands = load_brands();
    if let Some(b) = brands.iter_mut().find(|b| b.id == id) {
        b.name = body.name;
        b.main_server = body.main_server;
        b.app_username = body.app_username;
        b.app_userkey = body.app_userkey;
        let updated = b.clone();
        let _ = save_brands(&brands);
        (StatusCode::OK, Json(updated)).into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

async fn delete_brand_handler(Path(id): Path<String>) -> impl IntoResponse {
    let mut brands = load_brands();
    let before = brands.len();
    brands.retain(|b| b.id != id);
    if brands.len() == before {
        return StatusCode::NOT_FOUND;
    }
    let _ = save_brands(&brands);
    StatusCode::NO_CONTENT
}

async fn test_brand_handler(Json(body): Json<BrandPayload>) -> impl IntoResponse {
    match probe_provider(&body.main_server, &body.app_username, &body.app_userkey).await {
        Ok(message) => (
            StatusCode::OK,
            Json(serde_json::json!({"status": "ok", "message": message})),
        )
            .into_response(),
        Err(error) => (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({"status": "error", "error": error})),
        )
            .into_response(),
    }
}

/* ─── Camera handlers ─── */

async fn get_cameras() -> impl IntoResponse {
    Json(load_cameras())
}

#[derive(Deserialize)]
struct CameraPayload {
    name: String,
    brand: String,
    serial: String,
    username: String,
    password: String,
    port: u16,
    local_port: u16,
    #[serde(default)]
    auto_start: bool,
}

async fn create_camera(Json(body): Json<CameraPayload>) -> impl IntoResponse {
    let mut cameras = load_cameras();
    let camera = Camera {
        id: Uuid::new_v4().to_string(),
        name: body.name,
        brand: body.brand,
        serial: body.serial,
        username: body.username,
        password: body.password,
        port: body.port,
        local_port: body.local_port,
        auto_start: body.auto_start,
    };
    cameras.push(camera.clone());
    if save_cameras(&cameras).is_err() {
        return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to save").into_response();
    }
    (StatusCode::CREATED, Json(camera)).into_response()
}

async fn update_camera(
    Path(id): Path<String>,
    Json(body): Json<CameraPayload>,
) -> impl IntoResponse {
    let mut cameras = load_cameras();
    if let Some(cam) = cameras.iter_mut().find(|c| c.id == id) {
        cam.name = body.name;
        cam.brand = body.brand;
        cam.serial = body.serial;
        cam.username = body.username;
        cam.password = body.password;
        cam.port = body.port;
        cam.local_port = body.local_port;
        cam.auto_start = body.auto_start;
        let updated = cam.clone();
        let _ = save_cameras(&cameras);
        (StatusCode::OK, Json(updated)).into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Camera not found"})),
        )
            .into_response()
    }
}

async fn delete_camera(State(state): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    state.tunnel_manager.stop(&id);
    let mut cameras = load_cameras();
    let before = cameras.len();
    cameras.retain(|c| c.id != id);
    if cameras.len() == before {
        return StatusCode::NOT_FOUND;
    }
    let _ = save_cameras(&cameras);
    StatusCode::NO_CONTENT
}

async fn start_tunnel(State(state): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    let camera_and_provider = match &state.storage {
        Some(storage) => match storage.find_camera_config(&id).await {
            Ok(Some(camera)) => {
                let provider = storage
                    .find_provider_config(&camera.brand)
                    .await
                    .ok()
                    .flatten()
                    .map(brand_from_storage);
                Some((camera_from_storage(camera), provider))
            }
            _ => None,
        },
        None => load_cameras()
            .into_iter()
            .find(|camera| camera.id == id)
            .map(|camera| (camera, None)),
    };
    match camera_and_provider {
        Some((camera, provider)) => match state.tunnel_manager.start_with_brand(camera, provider) {
            Ok(_) => StatusCode::OK.into_response(),
            Err(e) => (StatusCode::CONFLICT, Json(serde_json::json!({"error": e}))).into_response(),
        },
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn stop_tunnel(State(state): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    state.tunnel_manager.stop(&id);
    state.live_manager.stop(&id);
    StatusCode::OK
}

#[derive(Serialize)]
struct TunnelInfo {
    id: String,
    status: String,
    rtsp_url: Option<String>,
}

async fn get_tunnel_statuses(State(state): State<AppState>) -> impl IntoResponse {
    let cameras = load_cameras();
    let infos: Vec<TunnelInfo> = cameras
        .iter()
        .map(|cam| {
            let (status_str, rtsp_url) = match state.tunnel_manager.status(&cam.id) {
                TunnelStatus::Running => (
                    "running".to_string(),
                    Some(format!(
                        "rtsp://{}:{}@127.0.0.1:{}/cam/realmonitor?channel=1&subtype=0",
                        cam.username, cam.password, cam.local_port
                    )),
                ),
                TunnelStatus::Starting => ("starting".to_string(), None),
                TunnelStatus::Error(e) => (format!("error: {}", e), None),
                TunnelStatus::Stopped => ("stopped".to_string(), None),
            };
            TunnelInfo {
                id: cam.id.clone(),
                status: status_str,
                rtsp_url,
            }
        })
        .collect();
    Json(infos)
}

#[derive(Serialize)]
struct CameraWithRtsp {
    id: String,
    name: String,
    brand: String,
    serial: String,
    local_port: u16,
    auto_start: bool,
    rtsp: String,
}

async fn get_cameras_all(State(state): State<AppState>) -> impl IntoResponse {
    let cameras = load_cameras();
    let result: Vec<CameraWithRtsp> = cameras
        .iter()
        .map(|cam| {
            let rtsp = match state.tunnel_manager.status(&cam.id) {
                TunnelStatus::Running => format!(
                    "rtsp://{}:{}@127.0.0.1:{}/cam/realmonitor?channel=1&subtype=0",
                    cam.username, cam.password, cam.local_port
                ),
                _ => String::new(),
            };
            CameraWithRtsp {
                id: cam.id.clone(),
                name: cam.name.clone(),
                brand: cam.brand.clone(),
                serial: cam.serial.clone(),
                local_port: cam.local_port,
                auto_start: cam.auto_start,
                rtsp,
            }
        })
        .collect();
    Json(result)
}

/* ─── Token handlers ─── */

async fn get_tokens_handler() -> impl IntoResponse {
    Json(load_tokens())
}

#[derive(Deserialize)]
struct TokenPayload {
    name: String,
    expires_at: Option<String>,
    enabled: bool,
}

async fn create_token_handler(Json(body): Json<TokenPayload>) -> impl IntoResponse {
    let mut tokens = load_tokens();
    let t = ApiToken {
        id: Uuid::new_v4().to_string(),
        name: body.name,
        token: format!("camrelay_{}", Uuid::new_v4().to_string().replace('-', "")),
        expires_at: body.expires_at,
        enabled: body.enabled,
    };
    tokens.push(t.clone());
    let _ = save_tokens(&tokens);
    (StatusCode::CREATED, Json(t))
}

async fn update_token_handler(
    Path(id): Path<String>,
    Json(body): Json<TokenPayload>,
) -> impl IntoResponse {
    let mut tokens = load_tokens();
    if let Some(t) = tokens.iter_mut().find(|t| t.id == id) {
        t.name = body.name;
        t.expires_at = body.expires_at;
        t.enabled = body.enabled;
        let updated = t.clone();
        let _ = save_tokens(&tokens);
        (StatusCode::OK, Json(updated)).into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

async fn delete_token_handler(Path(id): Path<String>) -> impl IntoResponse {
    let mut tokens = load_tokens();
    let before = tokens.len();
    tokens.retain(|t| t.id != id);
    if tokens.len() == before {
        return StatusCode::NOT_FOUND;
    }
    let _ = save_tokens(&tokens);
    StatusCode::NO_CONTENT
}

/* ─── Router ─── */

#[derive(Deserialize, Default)]
struct RecordingsQuery {
    camera_id: Option<String>,
    status: Option<String>,
    limit: Option<usize>,
}

async fn get_recordings_handler(
    State(state): State<AppState>,
    Query(query): Query<RecordingsQuery>,
) -> impl IntoResponse {
    Json(state.recording_manager.list_filtered(
        query.camera_id.as_deref(),
        query.status.as_deref(),
        query.limit.unwrap_or(100),
    ))
}

async fn get_recording_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.recording_manager.get(&id) {
        Some(recording) => (StatusCode::OK, Json(recording)).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Recording not found"})),
        )
            .into_response(),
    }
}

#[derive(Serialize)]
struct RecordingConfigResponse {
    enabled: bool,
    live_enabled: bool,
    live_protocol: String,
    live_protocol_available: bool,
    archive_enabled: bool,
    archive_verify: bool,
    archive_configured: bool,
    segment_seconds: u32,
    local_retention_days: u32,
    ffmpeg_path: String,
    checksum_algorithm: &'static str,
}

fn recording_config_response(state: &AppState) -> RecordingConfigResponse {
    let config = state.recording_manager.config();
    RecordingConfigResponse {
        enabled: config.recordings_enabled,
        live_enabled: config.live_enabled,
        live_protocol: state.live_manager.protocol(),
        live_protocol_available: state.live_manager.protocol_available(),
        archive_enabled: config.archive_enabled,
        archive_verify: config.archive_verify,
        archive_configured: !config.archive_remote.trim().is_empty(),
        segment_seconds: config.segment_seconds,
        local_retention_days: config.local_retention_days,
        ffmpeg_path: config.ffmpeg_path.clone(),
        checksum_algorithm: "sha256",
    }
}

async fn get_recording_config(State(state): State<AppState>) -> impl IntoResponse {
    Json(recording_config_response(&state))
}

async fn get_v1_recording_config(State(state): State<AppState>) -> impl IntoResponse {
    Json(recording_config_response(&state))
}

async fn get_v1_retention_preview(State(state): State<AppState>) -> impl IntoResponse {
    Json(state.recording_manager.retention_preview())
}

async fn archive_recording_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.recording_manager.archive(&id).await {
        Ok(recording) => (StatusCode::OK, Json(recording)).into_response(),
        Err(error) => (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({"error": error})),
        )
            .into_response(),
    }
}

#[derive(Serialize)]
struct PlaybackTicketResponse {
    url: String,
    expires_in_seconds: u64,
}

async fn issue_playback_ticket(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if state.recording_manager.get(&id).is_none() {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Recording not found"})),
        )
            .into_response();
    }
    let ticket = Uuid::new_v4().to_string();
    state.playback_tickets.lock().unwrap().insert(
        ticket.clone(),
        PlaybackGrant {
            recording_id: id.clone(),
            expires_at: Instant::now() + Duration::from_secs(600),
        },
    );
    (
        StatusCode::OK,
        Json(PlaybackTicketResponse {
            url: format!("/api/playback/{}?ticket={}", id, ticket),
            expires_in_seconds: 600,
        }),
    )
        .into_response()
}

#[derive(Serialize)]
struct ThumbnailTicketResponse {
    url: String,
    expires_in_seconds: u64,
}

async fn issue_thumbnail_ticket(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let Some(recording) = state.recording_manager.get(&id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !FsPath::new(&recording.local_path).is_file() {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "code": "recording.thumbnail_unavailable",
                "message": "A local recording file is required to generate a thumbnail."
            })),
        )
            .into_response();
    }
    let ticket = Uuid::new_v4().to_string();
    let expires_in_seconds = 600;
    state.playback_tickets.lock().unwrap().insert(
        ticket.clone(),
        PlaybackGrant {
            recording_id: id.clone(),
            expires_at: Instant::now() + Duration::from_secs(expires_in_seconds),
        },
    );
    (
        StatusCode::OK,
        Json(ThumbnailTicketResponse {
            url: format!("/api/v1/thumbnails/{id}?ticket={ticket}"),
            expires_in_seconds,
        }),
    )
        .into_response()
}

#[derive(Serialize)]
struct LiveTicketResponse {
    protocol: String,
    url: String,
    expires_in_seconds: u64,
}

async fn issue_live_ticket(
    State(state): State<AppState>,
    principal: Option<Extension<AuthPrincipal>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if principal.is_none() {
        return unauthorized_response();
    }
    if !state.live_manager.config().live_enabled {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "code": "live.disabled",
                "message": "Live media is disabled in config.json."
            })),
        )
            .into_response();
    }
    if !state.live_manager.protocol_available() {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "code": "live.protocol_unavailable",
                "message": "The configured live protocol is not available yet. Set live_protocol to hls."
            })),
        )
            .into_response();
    }
    let camera = match find_camera_config(&state, &id).await {
        Ok(Some(camera)) => camera,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "code": "live.camera_load_failed",
                    "message": error
                })),
            )
                .into_response()
        }
    };
    if !matches!(
        state.tunnel_manager.status(&camera.id),
        TunnelStatus::Running
    ) {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "code": "live.tunnel_required",
                "message": "Start the camera relay before requesting live playback."
            })),
        )
            .into_response();
    }
    if let Err(error) = state.live_manager.start(camera).await {
        return (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({
                "code": "live.gateway_start_failed",
                "message": error
            })),
        )
            .into_response();
    }

    let ticket = Uuid::new_v4().to_string();
    let expires_in_seconds = 900;
    let mut tickets = state.live_tickets.lock().unwrap();
    let now = Instant::now();
    tickets.retain(|_, grant| grant.expires_at > now);
    tickets.insert(
        ticket.clone(),
        LiveGrant {
            camera_id: id,
            expires_at: now + Duration::from_secs(expires_in_seconds),
        },
    );
    (
        StatusCode::OK,
        Json(LiveTicketResponse {
            protocol: state.live_manager.protocol(),
            url: format!("/api/v1/live/{ticket}/index.m3u8"),
            expires_in_seconds,
        }),
    )
        .into_response()
}

async fn stream_live_handler(
    State(state): State<AppState>,
    Path((ticket, file)): Path<(String, String)>,
) -> Response {
    let grant = {
        let tickets = state.live_tickets.lock().unwrap();
        tickets
            .get(&ticket)
            .filter(|grant| grant.expires_at > Instant::now())
            .cloned()
    };
    let Some(grant) = grant else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "code": "live.ticket_invalid",
                "message": "Live ticket expired or invalid."
            })),
        )
            .into_response();
    };
    if !safe_live_component(&grant.camera_id) || !safe_live_file(&file) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let path = FsPath::new(&state.live_manager.config().live_dir)
        .join(grant.camera_id)
        .join(&file);
    let content = match tokio::fs::read(path).await {
        Ok(content) => content,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    let content_type = if file.ends_with(".m3u8") {
        "application/vnd.apple.mpegurl"
    } else {
        "video/mp2t"
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from(content))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

fn safe_live_component(value: &str) -> bool {
    !value.is_empty()
        && !value.contains('/')
        && !value.contains('\\')
        && value != "."
        && value != ".."
}

fn safe_live_file(value: &str) -> bool {
    safe_live_component(value) && (value.ends_with(".m3u8") || value.ends_with(".ts"))
}

async fn playback_auth_middleware(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Response {
    let ticket = req.uri().query().and_then(|query| {
        query.split('&').find_map(|part| {
            let (key, value) = part.split_once('=')?;
            (key == "ticket").then(|| value.to_string())
        })
    });
    let recording_id = req.uri().path().rsplit('/').next().unwrap_or_default();
    let valid = ticket
        .and_then(|ticket| {
            let grants = state.playback_tickets.lock().unwrap();
            grants
                .get(&ticket)
                .filter(|grant| {
                    grant.recording_id == recording_id && grant.expires_at > Instant::now()
                })
                .cloned()
        })
        .is_some();
    if valid {
        next.run(req).await
    } else {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "Playback ticket expired or invalid"})),
        )
            .into_response()
    }
}

fn parse_range(value: Option<&HeaderValue>, size: u64) -> Result<Option<(u64, u64)>, StatusCode> {
    let Some(value) = value else {
        return Ok(None);
    };
    let text = value
        .to_str()
        .map_err(|_| StatusCode::RANGE_NOT_SATISFIABLE)?;
    let Some(spec) = text
        .strip_prefix("bytes=")
        .and_then(|value| value.split(',').next())
    else {
        return Err(StatusCode::RANGE_NOT_SATISFIABLE);
    };
    let (start_text, end_text) = spec
        .split_once('-')
        .ok_or(StatusCode::RANGE_NOT_SATISFIABLE)?;
    let start = start_text
        .parse::<u64>()
        .map_err(|_| StatusCode::RANGE_NOT_SATISFIABLE)?;
    if start >= size {
        return Err(StatusCode::RANGE_NOT_SATISFIABLE);
    }
    let end = if end_text.is_empty() {
        size - 1
    } else {
        end_text
            .parse::<u64>()
            .map_err(|_| StatusCode::RANGE_NOT_SATISFIABLE)?
            .min(size - 1)
    };
    if end < start {
        return Err(StatusCode::RANGE_NOT_SATISFIABLE);
    }
    Ok(Some((start, end)))
}

async fn stream_recording_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    request: Request,
) -> Response {
    let Some(recording) = state.recording_manager.get(&id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let range = match parse_range(request.headers().get(header::RANGE), recording.bytes) {
        Ok(range) => range,
        Err(status) => return status.into_response(),
    };
    let (start, end) = range.unwrap_or((0, recording.bytes.saturating_sub(1)));
    let length = end.saturating_sub(start).saturating_add(1);
    let mut response;

    if FsPath::new(&recording.local_path).is_file() {
        let Ok(mut file) = File::open(&recording.local_path).await else {
            return StatusCode::NOT_FOUND.into_response();
        };
        if file.seek(SeekFrom::Start(start)).await.is_err() {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
        response = Response::new(Body::from_stream(ReaderStream::new(file.take(length))));
    } else {
        let Some(target) = state.recording_manager.archive_target(&recording) else {
            return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Recording is not available locally and no archive is configured"}))).into_response();
        };
        let mut command = Command::new("rclone");
        command.arg("cat").arg(target).stdout(Stdio::piped());
        if start > 0 {
            command.arg("--offset").arg(start.to_string());
        }
        if range.is_some() {
            command.arg("--count").arg(length.to_string());
        }
        let Ok(mut child) = command.spawn() else {
            return (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({"error": "Could not start rclone for remote playback"})),
            )
                .into_response();
        };
        let Some(stdout) = child.stdout.take() else {
            return StatusCode::BAD_GATEWAY.into_response();
        };
        tokio::spawn(async move {
            let _ = child.wait().await;
        });
        response = Response::new(Body::from_stream(ReaderStream::new(stdout)));
    }

    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("video/mp4"));
    headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    if let Ok(value) = HeaderValue::from_str(&length.to_string()) {
        headers.insert(header::CONTENT_LENGTH, value);
    }
    if range.is_some() {
        if let Ok(value) =
            HeaderValue::from_str(&format!("bytes {}-{}/{}", start, end, recording.bytes))
        {
            headers.insert(header::CONTENT_RANGE, value);
        }
        *response.status_mut() = StatusCode::PARTIAL_CONTENT;
    }
    response
}

async fn stream_thumbnail_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    let thumbnail = match state.recording_manager.ensure_thumbnail(&id).await {
        Ok(path) => path,
        Err(error) if error == "Recording not found" => {
            return StatusCode::NOT_FOUND.into_response()
        }
        Err(error) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({
                    "code": "recording.thumbnail_failed",
                    "message": error
                })),
            )
                .into_response()
        }
    };
    let content = match tokio::fs::read(thumbnail).await {
        Ok(content) => content,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "image/jpeg")
        .header(header::CACHE_CONTROL, "private, no-store")
        .body(Body::from(content))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
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

pub fn create_router(state: AppState) -> Router {
    let web_root = state.config.web_root.clone();
    let web_index = format!("{web_root}/index.html");
    let protected = Router::new()
        .route(
            "/brands",
            get(get_brands_handler).post(create_brand_handler),
        )
        .route("/brands/test", post(test_brand_handler))
        .route(
            "/brands/:id",
            put(update_brand_handler).delete(delete_brand_handler),
        )
        .route("/cameras", get(get_cameras).post(create_camera))
        .route("/cameras/all", get(get_cameras_all))
        .route("/cameras/:id", put(update_camera).delete(delete_camera))
        .route("/cameras/:id/start", post(start_tunnel))
        .route("/cameras/:id/stop", post(stop_tunnel))
        .route("/tunnels", get(get_tunnel_statuses))
        .route(
            "/tokens",
            get(get_tokens_handler).post(create_token_handler),
        )
        .route(
            "/tokens/:id",
            put(update_token_handler).delete(delete_token_handler),
        )
        .route("/recordings", get(get_recordings_handler))
        .route("/recordings/config", get(get_recording_config))
        .route("/recordings/:id", get(get_recording_handler))
        .route("/recordings/:id/archive", post(archive_recording_handler))
        .route(
            "/recordings/:id/playback-ticket",
            post(issue_playback_ticket),
        )
        .route(
            "/recordings/:id/thumbnail-ticket",
            post(issue_thumbnail_ticket),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    let login_router = Router::new()
        .route("/login", post(login))
        .route("/logout", post(logout));
    let playback_router = Router::new()
        .route("/playback/:id", get(stream_recording_handler))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            playback_auth_middleware,
        ));
    let v1_thumbnail_router = Router::new()
        .route("/thumbnails/:id", get(stream_thumbnail_handler))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            playback_auth_middleware,
        ));
    let v1_public = Router::new()
        .route("/health", get(v1_health))
        .route("/system/health", get(v1_health))
        .route("/system/readiness", get(v1_readiness))
        .route("/live/:ticket/*file", get(stream_live_handler));
    let v1_auth = Router::new()
        .route("/auth/login", post(login))
        .route("/auth/refresh", post(refresh_session))
        .route("/auth/logout", post(logout))
        .route("/me", get(current_user));
    let v1_protected = Router::new()
        .route("/users", get(get_v1_users).post(create_v1_user))
        .route("/audit", get(get_v1_audit))
        .route("/system/stream", get(system_stream))
        .route("/cameras", get(get_v1_cameras).post(create_v1_camera))
        .route(
            "/cameras/:id",
            get(get_v1_camera)
                .patch(update_v1_camera)
                .delete(delete_v1_camera),
        )
        .route("/cameras/:id/diagnostics", get(get_v1_camera_diagnostics))
        .route("/cameras/:id/live-ticket", post(issue_live_ticket))
        .route("/providers", get(get_v1_providers).post(create_v1_provider))
        .route(
            "/providers/:id",
            get(get_v1_provider)
                .patch(update_v1_provider)
                .delete(delete_v1_provider),
        )
        .route("/recordings", get(get_v1_recordings))
        .route("/recordings/config", get(get_v1_recording_config))
        .route(
            "/recordings/retention-preview",
            get(get_v1_retention_preview),
        )
        .route("/events", get(get_v1_events))
        .route("/recordings/:id", get(get_v1_recording))
        .route(
            "/recordings/:id/playback-ticket",
            post(issue_playback_ticket),
        )
        .route(
            "/recordings/:id/thumbnail-ticket",
            post(issue_thumbnail_ticket),
        )
        .route("/recordings/:id/archive", post(archive_v1_recording))
        .route("/tokens", get(get_v1_tokens).post(create_v1_token))
        .route(
            "/tokens/:id",
            patch(update_v1_token).delete(delete_v1_token),
        )
        .route("/tunnels", get(get_v1_tunnels))
        .route("/cameras/:id/start", post(start_tunnel))
        .route("/cameras/:id/stop", post(stop_tunnel))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    Router::new()
        .nest("/api", protected)
        .nest("/api", login_router)
        .nest("/api", playback_router)
        .nest("/api/v1", v1_public)
        .nest("/api/v1", v1_auth)
        .nest("/api/v1", v1_thumbnail_router)
        .nest("/api/v1", v1_protected)
        // `fallback` preserves the index file's 200 status for SPA deep links;
        // `not_found_service` would return the right body with a misleading 404.
        .fallback_service(ServeDir::new(web_root).fallback(ServeFile::new(web_index)))
        .with_state(state)
}
