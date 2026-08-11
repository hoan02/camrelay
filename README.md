# camrelay

Self-hosted P2P camera relay written in Rust. `camrelay` exposes an authorized remote Dahua-family P2P camera or NVR as a local TCP endpoint that an RTSP client, FFmpeg, Frigate, or another NVR can consume.

The project currently contains a multi-camera manager, a web dashboard, a REST API, JSON-backed configuration, and a Dahua P2P/PTCP tunnel implementation. Compatibility with a specific vendor, model, firmware, or cloud app must be verified against a real device; this repository does not claim IMOU compatibility without a successful source-level or live-device test.

## v1 foundation status

The `v1-foundation` branch is an incremental migration from the known `prototype-v0.1` relay baseline. The Rust binary and `static/` console remain the rollback path while the new API-first boundaries are verified:

- Rust is now a Cargo workspace with a transport-independent `camrelay-contract` crate.
- `apps/web` contains the new React/TypeScript console foundation with real browser routes, design tokens, dark/light mode, persistent English/Vietnamese settings, and an API client.
- `apps/mobile` contains the Flutter companion scaffold using the same versioned API and platform secure storage for bearer tokens; Flutter build verification is pending until the SDK is installed.
- `packages/design-tokens` is the first shared visual language seam for the future web and mobile clients.
- `apps/web` is the feature-parity v1 console for development and can be served by the Rust binary by setting `web_root` to `apps/web/dist`; the legacy console remains available as a rollback path.

## Overview

The service connects to a camera through a configured P2P cloud endpoint, establishes a PTCP session, and listens on a local TCP port. RTSP clients connect to that local port using the camera's normal RTSP path.

```mermaid
flowchart LR
    UI[Web dashboard or API client] --> M[camrelay manager]
    M <-->|Dahua P2P / PTCP over UDP| C[Configured P2P cloud service]
    C <-->|P2P signaling| D[Remote camera or NVR]
    F[Frigate / FFmpeg / VLC] -->|Local TCP RTSP| M
    M -->|One local port per camera| F
```

## Architecture

- `src/main.rs` loads configuration, auto-starts cameras, and starts the Axum web server.
- `src/config.rs` defines the JSON-backed application, brand, camera, and API-token models.
- `src/web.rs` provides login, camera/brand/token CRUD, tunnel controls, status endpoints, and static-file serving.
- `src/tunnel.rs` owns one asynchronous tunnel task per camera and accepts local TCP clients.
- `src/dh.rs` implements the Dahua HTTP-like P2P signaling and handshake.
- `src/ptcp.rs` implements PTCP packet/session handling and multiplexed realms.
- `src/process.rs` bridges local TCP client data to PTCP and back.
- `src/recordings.rs` manages FFmpeg segment processes, recording metadata, rclone archiving, and playback source selection.
- `static/index.html`, `static/app.js`, and `static/app.css` provide the routed web manager.
- `dh-p2p.lua` is an optional Wireshark dissector for protocol investigation.

## Features

### Available in the current source

- Rust async service using Tokio and Axum.
- Multiple camera definitions and independent start/stop controls.
- One tunnel task and one local TCP port per camera.
- Multiple local clients multiplexed through PTCP realms for a camera.
- Web dashboard and bearer-token REST API.
- Direct browser routes for login, dashboard, cameras, providers, tokens, settings, and technical guide.
- Dark/light theme, English/Vietnamese interface, and responsive control-room layout.
- Separate platform credentials from camera RTSP credentials in the setup flow.
- Owner-only user administration with role summaries and Argon2id password creation in SQLite mode.
- Provider probe before a camera can be added; the probe validates provider signaling only.
- Auto-start for cameras with `auto_start: true`.
- Brand-specific P2P server and app credentials.
- SQLite v1 persistence with encrypted provider/camera secrets, Argon2id user passwords, refresh sessions, RBAC, and idempotent import from legacy JSON files.
- Optional local HLS live preview through FFmpeg and a loopback-only RTSP credential proxy; camera secrets do not enter the FFmpeg command line or a sidecar service.
- Direct and relay handshake paths in the Rust implementation, subject to device/cloud support.

### Not yet promised

- Universal Dahua compatibility.
- IMOU compatibility.
- Automatic discovery or provisioning of cameras.
- TLS, encrypted configuration storage, or production-grade secret management.
- Guaranteed reconnection after every network or device failure.
- A native Frigate integration.
- Provider probe success does not guarantee camera, firmware, model, region, or RTSP compatibility.
- Long-term universal device compatibility, automatic discovery, native Frigate integration, and unattended production reliability are not promised yet.

## Build and run

Install a current stable Rust toolchain, then run from the repository directory:

```bash
cargo check
cargo run --release
```

The web manager listens on `0.0.0.0:<web_port>`. If `config.json` is absent, the service uses the built-in defaults. To create local runtime files from the sanitized examples:

```bash
cp config.example.json config.json
cp brands.example.json brands.json
cp cameras.example.json cameras.json
cp tokens.example.json tokens.json
```

Then open:

```text
http://127.0.0.1:8080
```

The default login is `admin` / `change-me` when using `config.example.json`; the built-in fallback without a config file is `admin` / `admin`. Change it before exposing the service beyond a trusted local machine.

The web manager has these browser routes:

| Route | Purpose |
| --- | --- |
| `/login` | Authentication screen. |
| `/dashboard` | Camera and tunnel overview. |
| `/cameras` | Camera setup and local RTSP endpoints. |
| `/recordings` | Closed-segment browser playback and archive status. |
| `/providers` | Platform provider profiles and provider probe. |
| `/tokens` | API token management. |
| `/users` | Owner-only appliance user and role management. |
| `/settings` | Language and dark/light theme. |
| `/about` | Architecture, protocol, security, and compatibility notes. |

Because routes are real paths, refreshing `/cameras` or `/providers` keeps that page selected.

### New React console (development)

Run the Rust service in one terminal, then start the new console from the repository root in another:

```bash
npm install
npm run web:dev
```

Open `http://127.0.0.1:5173/login`. Vite proxies `/api` to the Rust service on port `8080`. The console covers the v1 dashboard, camera/provider CRUD, lifecycle controls, recording playback/archive actions, API tokens, settings, and technical notes.

To serve the built console from the Rust binary locally:

```bash
npm run web:build
# set web_root in config.json to "apps/web/dist", or export
# CAMRELAY_WEB_ROOT=apps/web/dist
cargo run --release
```

Leave `web_root` as `static` to roll back to the legacy console.

### Flutter companion (foundation)

The mobile client lives in `apps/mobile` and targets the same `/api/v1`
contract. Install Flutter, then run `flutter pub get`, `flutter analyze`, and
`flutter test` from that directory. The app stores its bearer token in platform
secure storage and never receives provider or camera credentials.

The service reads JSON files from its current working directory. Missing `config.json` falls back to the built-in admin/port defaults; brands, cameras, and tokens fall back to empty lists.

## Configuration

### `config.json`

```json
{
  "username": "admin",
  "password": "change-me",
  "web_port": 8080
}
```

### `brands.json`

Each brand supplies the signaling endpoint and platform application credentials required by the P2P handshake. These are not the username/password of the camera and must come from an authorized, working configuration.

```json
[
  {
    "id": "brand-1",
    "name": "Dahua",
    "main_server": "www.easy4ipcloud.com:8800",
    "app_username": "REPLACE_ME",
    "app_userkey": "REPLACE_ME"
  }
]
```

The camera's `brand` value must match `name`. Brands can also be managed from the dashboard or `/api/brands`.

The dashboard separates these fields as **Platform credentials**. Use **Test provider** before adding a camera. The probe sends the platform credential to `/probe/p2psrv` and confirms that the configured endpoint accepts it. A successful probe only verifies provider-level signaling; it does not prove that a specific camera, model, firmware, or RTSP path is compatible.

### `cameras.json`

```json
[
  {
    "id": "cam-1",
    "name": "Front door",
    "brand": "Dahua",
    "serial": "DEVICE_SERIAL",
    "username": "admin",
    "password": "DEVICE_RTSP_PASSWORD",
    "port": 554,
    "local_port": 8551,
    "auto_start": true
  }
]
```

- `serial` is the camera/NVR P2P serial number.
- `username` and `password` are device RTSP credentials, not cloud app credentials.
- `port` is the remote RTSP port, normally `554`.
- `local_port` is the local TCP port exposed by `camrelay`; every camera needs a different value.
- `auto_start` starts the tunnel when the service boots.

### `tokens.json`

API tokens are bearer credentials. Keep them private and do not commit real values:

```json
[
  {
    "id": "token-1",
    "name": "Frigate",
    "token": "camrelay_REPLACE_ME",
    "expires_at": null,
    "enabled": true
  }
]
```

## RTSP usage and testing

Start a camera from the dashboard or API and wait until its tunnel status is `running`. Then test the local endpoint with FFmpeg's `ffplay`:

```bash
ffplay -rtsp_transport tcp \
  "rtsp://admin:DEVICE_RTSP_PASSWORD@127.0.0.1:8551/cam/realmonitor?channel=1&subtype=0"
```

The local port is the configured `local_port`; the RTSP path and channel can vary by device. Use the exact path required by the camera/NVR.

For Frigate or another NVR, point the input at the same local RTSP URL. Ensure the relay host can reach the local port and that the service remains running.

## Recording, playback, and Google Drive archive

Recording is an optional operational layer around the relay:

    camera P2P -> camrelay local RTSP -> FFmpeg segments -> recordings.json
                                                 |
                                                 +-> rclone -> Google Drive

camrelay does not make Google Drive a live-stream origin. Live RTSP is still intended for Frigate, go2rtc, FFmpeg, or another local media consumer. The Recordings page is for closed segments and supports browser playback from local storage or the configured archive.

Enable recording in config.json:

    {
      "recordings_enabled": true,
      "live_enabled": true,
      "live_dir": "live",
      "recordings_dir": "recordings",
      "recordings_index": "recordings.json",
      "segment_seconds": 300,
      "ffmpeg_path": "ffmpeg",
      "archive_enabled": true,
      "archive_remote": "gdrive:camrelay-archive",
      "archive_root": "camrelay-archive",
      "archive_poll_seconds": 15,
      "local_retention_days": 0,
      "database_enabled": false,
      "database_path": "camrelay.sqlite"
    }

Install FFmpeg and make sure ffmpeg is on PATH, or set an absolute ffmpeg_path. The recorder starts only while the camera tunnel is running. It writes one MP4 segment per camera and indexes a segment after it has been closed.

For Google Drive, configure an rclone remote locally:

    rclone config
    rclone lsd gdrive:
    rclone mkdir gdrive:camrelay-archive

Set archive_remote to the configured remote name and enable archive_enabled. Uploads use rclone copyto; no Google OAuth secret or service-account key belongs in this repository. The Archive now action retries one segment on demand, while the background worker uploads new local segments automatically.

The browser requests a short-lived playback ticket for one recording. The ticket is scoped to that recording and expires after ten minutes; the main dashboard bearer token is not placed in a video URL. The stream endpoint supports Range so the browser can seek. If the local file has been removed and the recording is archived, camrelay invokes rclone cat for playback.

To enable browser live preview, set `live_enabled` to `true`, install FFmpeg, start the camera relay, and press Live on the camera card. Camrelay creates short-lived HLS segments under `live_dir`, serves them through a camera-scoped ticket, and uses an in-process RTSP auth proxy so the camera username/password stays inside camrelay. HLS playback support depends on the browser; WebRTC remains a later gateway phase.

The SQLite v1 foundation is opt-in for now. Set `database_enabled` to `true` to create the configured database, apply SQLx migrations, and import any legacy JSON files idempotently. Before enabling it when providers or cameras exist, set `CAMRELAY_SECRET_KEY` to a base64-encoded 32-byte key (see `.env.example`). User passwords are Argon2id hashes; provider and camera secrets are encrypted with ChaCha20-Poly1305 and the master key must be backed up separately. In SQLite mode, v1 camera/provider reads, camera onboarding, tunnel startup, auto-start, recording reconciliation, auth, and token management use SQLite. Legacy JSON handlers remain available only as a rollback path; the database file is ignored by Git.

For a first test, leave archive_enabled false, enable recording, start one camera, wait for one segment to close, and open /recordings. Then configure rclone and archive the same segment.

## Multi-camera example

Each camera uses its own local port:

```json
[
  {
    "id": "cam-1",
    "name": "Entrance",
    "brand": "Dahua",
    "serial": "SERIAL_1",
    "username": "admin",
    "password": "PASSWORD_1",
    "port": 554,
    "local_port": 8551,
    "auto_start": true
  },
  {
    "id": "cam-2",
    "name": "Garage",
    "brand": "Dahua",
    "serial": "SERIAL_2",
    "username": "admin",
    "password": "PASSWORD_2",
    "port": 554,
    "local_port": 8552,
    "auto_start": true
  },
  {
    "id": "cam-3",
    "name": "Back yard",
    "brand": "Dahua",
    "serial": "SERIAL_3",
    "username": "admin",
    "password": "PASSWORD_3",
    "port": 554,
    "local_port": 8553,
    "auto_start": true
  }
]
```

The corresponding test URLs use ports `8551`, `8552`, and `8553`.

## Web manager and API

The web manager is served by the same Rust process. API routes are under `/api` and require `Authorization: Bearer <token>`, except `POST /api/login`.

| Method | Path | Purpose |
| --- | --- | --- |
| POST | `/api/login` | Create an in-memory session token. |
| GET/POST | `/api/brands` | List or create brand configurations. |
| POST | `/api/brands/test` | Probe a provider endpoint with platform credentials; does not test a camera or RTSP. |
| PUT/DELETE | `/api/brands/:id` | Update or delete a brand. |
| GET/POST | `/api/cameras` | List or create cameras. |
| PUT/DELETE | `/api/cameras/:id` | Update or delete a camera. |
| POST | `/api/cameras/:id/start` | Start a tunnel. |
| POST | `/api/cameras/:id/stop` | Stop a tunnel. |
| GET | `/api/tunnels` | Read tunnel status and RTSP URLs. |
| GET/POST | `/api/tokens` | List or issue API tokens. |
| PUT/DELETE | `/api/tokens/:id` | Update or revoke an API token. |
| GET | `/api/recordings` | List indexed recording segments with optional camera/status filters. |
| GET | `/api/recordings/config` | Read safe recording/archive capability settings. |
| POST | `/api/recordings/:id/playback-ticket` | Issue a short-lived ticket for one recording. |
| GET | `/api/playback/:id?ticket=...` | Stream local or archived MP4 with Range support. |
| POST | `/api/recordings/:id/archive` | Upload one segment through the configured rclone remote. |

## Security notes

- Change the default dashboard password immediately.
- Treat `config.json`, `brands.json`, `cameras.json`, and `tokens.json` as secrets-bearing files.
- Do not expose the web server directly to the internet; place it behind a firewall and, if needed, a TLS reverse proxy.
- Use API tokens with an expiry and the smallest practical access scope once more granular authorization exists.
- Use only credentials and P2P access for devices you own or are authorized to administer.
- Review vendor terms, privacy requirements, and local law before using cloud relay traffic or recording remote streams.

## Limitations

- The current source has no automated integration test against a real camera.
- Device, firmware, region, cloud-account, and brand-app differences can change P2P behavior.
- A device that requires authentication while creating the P2P channel is reported by the handshake but is not fully supported.
- Relay mode exists in the handshake code but is not exposed as a per-camera setting.
- The web server has no built-in TLS.
- Runtime JSON is plaintext and writes are not transactional.
- Recording files are local plaintext media; local_retention_days defaults to 0 (never delete automatically).
- Provider probe state is kept in the current browser session and must be repeated after a reload or service restart.
- Error handling and reconnect behavior still need hardening for unattended NVR use.

## Roadmap

Completed in the v1 foundation:

- versioned API contract, SQLite persistence, encrypted device/provider secrets, auth sessions, RBAC, token management, readiness, and CRUD;
- React console routes for dashboard/live wall, cameras, providers, filtered recordings/timeline, tokens, settings, audit history, and technical notes;
- local recording playback tickets, Range streaming, archive actions, privacy-safe audit history, backup/restore scripts, and Compose configuration.

Next, in order:

1. Make the React build the default production console after a final migration review; keep `static/` as an explicit rollback option.
2. Add a WebRTC media gateway (MediaMTX, go2rtc, or an in-process alternative) behind the existing credential-free local proxy; HLS is already available for web/mobile and Frigate/FFmpeg integrations.
3. Add recording retention, upload checksums/resume, thumbnails, and event markers.
4. Validate supported device families with authorized hardware and document results by model, firmware, region, and provider profile; IMOU remains unclaimed until source-level or live-device evidence exists.
5. Validate ticket-backed playback on an authorized Android device and complete the iOS runner/device build on macOS; then add a WebRTC adapter when the gateway contract is finalized.

See [the media gateway design](docs/media-gateway.md) and [the authorized device validation matrix](docs/device-validation-matrix.md) for the next-phase contracts and evidence requirements.

## Protocol and investigation notes

`dh-p2p.lua` can be loaded into Wireshark when investigating Dahua HTTP-like signaling and PTCP traffic. PTCP is a proprietary protocol reconstructed through reverse engineering; official protocol documentation is not included here.

## Upstream attribution and license

This repository retains the upstream MIT license and attribution. The implementation and protocol investigation were inspired by:

- [mcw0/PoC](https://github.com/mcw0/PoC), for foundational handshake/PTCP structure.
- [p2p-sys](https://github.com/p2p-sys), for the STUN inversion concept.

See `LICENSE` for the applicable license text and retain attribution when redistributing derived work.
