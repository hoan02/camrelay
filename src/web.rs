use axum::{
    body::Body,
    extract::{Path, Query, Request, State},
    http::{header, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use chrono;
use serde::{Deserialize, Serialize};
use std::{
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

use crate::config::{
    load_brands, load_cameras, load_tokens, save_brands, save_cameras, save_tokens, ApiToken,
    AppConfig, Brand, Camera,
};
use crate::dh::probe_provider;
use crate::recordings::RecordingManager;
use crate::tunnel::{TunnelManager, TunnelStatus};

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub sessions: Arc<Mutex<Vec<String>>>,
    pub tunnel_manager: Arc<TunnelManager>,
    pub recording_manager: RecordingManager,
    pub playback_tickets: Arc<Mutex<std::collections::HashMap<String, PlaybackGrant>>>,
}

#[derive(Clone)]
pub struct PlaybackGrant {
    pub recording_id: String,
    pub expires_at: Instant,
}

async fn auth_middleware(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let token = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string());

    if let Some(t) = token {
        if state.sessions.lock().unwrap().contains(&t) {
            return next.run(req).await;
        }

        let now = chrono::Utc::now();
        let valid = load_tokens().into_iter().any(|at| {
            at.token == t
                && at.enabled
                && match at.expires_at {
                    None => true,
                    Some(exp) => chrono::DateTime::parse_from_rfc3339(&exp)
                        .map(|dt| dt.with_timezone(&chrono::Utc) > now)
                        .unwrap_or(false),
                }
        });

        if valid {
            return next.run(req).await;
        }
    }

    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({"error": "Unauthorized"})),
    )
        .into_response()
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
    if body.username == state.config.username && body.password == state.config.password {
        let token = Uuid::new_v4().to_string();
        state.sessions.lock().unwrap().push(token.clone());
        (StatusCode::OK, Json(LoginResponse { token })).into_response()
    } else {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "Invalid credentials"})),
        )
            .into_response()
    }
}

/* ─── Brand handlers ─── */

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
    let cameras = load_cameras();
    match cameras.into_iter().find(|c| c.id == id) {
        Some(camera) => match state.tunnel_manager.start(camera) {
            Ok(_) => StatusCode::OK.into_response(),
            Err(e) => (StatusCode::CONFLICT, Json(serde_json::json!({"error": e}))).into_response(),
        },
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn stop_tunnel(State(state): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    state.tunnel_manager.stop(&id);
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
    archive_enabled: bool,
    archive_configured: bool,
    segment_seconds: u32,
    local_retention_days: u32,
    ffmpeg_path: String,
}

async fn get_recording_config(State(state): State<AppState>) -> impl IntoResponse {
    let config = state.recording_manager.config();
    Json(RecordingConfigResponse {
        enabled: config.recordings_enabled,
        archive_enabled: config.archive_enabled,
        archive_configured: !config.archive_remote.trim().is_empty(),
        segment_seconds: config.segment_seconds,
        local_retention_days: config.local_retention_days,
        ffmpeg_path: config.ffmpeg_path.clone(),
    })
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

pub fn create_router(state: AppState) -> Router {
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
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    let login_router = Router::new().route("/login", post(login));
    let playback_router = Router::new()
        .route("/playback/:id", get(stream_recording_handler))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            playback_auth_middleware,
        ));

    Router::new()
        .nest("/api", protected)
        .nest("/api", login_router)
        .nest("/api", playback_router)
        .fallback_service(
            ServeDir::new("static").not_found_service(ServeFile::new("static/index.html")),
        )
        .with_state(state)
}
