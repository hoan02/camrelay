# API v1 contract outline

## Conventions

- Base path: /api/v1.
- JSON uses snake_case.
- Errors have a stable machine code, safe message, optional field errors, and request ID.
- Mutating requests require authentication and authorization.
- Realtime status starts with Server-Sent Events; WebSocket is added only when bidirectional control needs it.

## Initial resources

| Resource | Responsibilities |
| --- | --- |
| auth | login, refresh, logout, device sessions |
| users | account and role administration |
| providers | P2P profiles; secret fields are write-only |
| cameras | device configuration, capability, lifecycle and health |
| live | scoped WebRTC/HLS ticket and connection metadata |
| recordings | timeline, playback ticket, export and archive status |
| events | motion/system/event timeline |
| system | health, readiness, version, storage and diagnostics |

## First endpoint set

Implemented in the current migration checkpoint:

    GET    /api/v1/health                 public migration health
    GET    /api/v1/system/readiness       public deployment readiness
    GET    /api/v1/cameras                authenticated, secret-free summaries
    POST   /api/v1/cameras                authenticated camera onboarding
    GET    /api/v1/providers               authenticated, secret-free summaries
    POST   /api/v1/providers               authenticated provider onboarding
    GET    /api/v1/recordings              authenticated summaries; optional camera_id/status/limit filters
    GET    /api/v1/recordings/config       authenticated recording/archive capability state
    GET    /api/v1/recordings/retention-preview authenticated read-only retention calculation
    GET    /api/v1/recordings/{recording_id} authenticated, secret-free detail
    POST   /api/v1/recordings/{recording_id}/playback-ticket
    POST   /api/v1/recordings/{recording_id}/thumbnail-ticket
    GET    /api/v1/thumbnails/{recording_id}?ticket=... scoped local JPEG thumbnail
    POST   /api/v1/recordings/{recording_id}/archive
    GET    /api/v1/tunnels                 authenticated, secret-free lifecycle status
    GET    /api/v1/tokens                  authenticated, secret-free summaries
    POST   /api/v1/tokens                  authenticated token creation; secret returned once
    PATCH  /api/v1/tokens/{token_id}       authenticated token metadata update
    DELETE /api/v1/tokens/{token_id}       authenticated token revocation
    POST   /api/v1/cameras/{camera_id}/start
    POST   /api/v1/cameras/{camera_id}/stop
    GET    /api/v1/cameras/{camera_id}
    PATCH  /api/v1/cameras/{camera_id}
    DELETE /api/v1/cameras/{camera_id}
    GET    /api/v1/cameras/{camera_id}/diagnostics
    POST   /api/v1/cameras/{camera_id}/live-ticket
    GET    /api/v1/live/{ticket}/{file}     scoped HLS playlist/segment delivery
    GET    /api/v1/providers/{provider_id}
    PATCH  /api/v1/providers/{provider_id}
    DELETE /api/v1/providers/{provider_id}
    GET    /api/v1/events
    GET    /api/v1/audit                  authenticated owner/admin audit history
    GET    /api/v1/system/health
    GET    /api/v1/system/stream              authenticated Server-Sent Events telemetry
    POST   /api/v1/auth/login
    POST   /api/v1/auth/refresh
    POST   /api/v1/auth/logout
    GET    /api/v1/me
    GET    /api/v1/users                 owner-only, secret-free user summaries
    POST   /api/v1/users                 owner-only user creation

The live media transport currently exposes ticketed HLS. WebRTC connection
metadata remains a later media-gateway phase.

The events endpoint currently returns normalized recording/system activity. Each
recording segment is represented with its optional `recording_id`, camera,
timestamp, status message, and source. It is not a motion-detection feed until
an authorized camera event source is integrated.

Recording summaries include an optional `checksum_sha256` calculated from the
local MP4 bytes. Existing indexes are backfilled when a local file is next
seen; remote-provider verification is opt-in and resumable transfer policy
remains separate archive work.

`GET /api/v1/recordings/config` returns read-only media capability state. It
does not return the rclone remote name, OAuth material, or archive credentials.
When `archive_verify` is enabled, a completed upload is marked
`archive_verified: true` only after rclone downloads and hashes the remote
object with SHA-256; this is intentionally opt-in because it reads the full
object again.
`GET /api/v1/recordings/retention-preview` calculates old local segments without
deleting anything. `local_retention_days: 0` produces an empty preview.
`auto_delete_enabled` is currently always `false`; when archive is enabled,
local segments without an archived status are counted as blocked rather than
eligible.

## Roles

| Role | Scope |
| --- | --- |
| owner | appliance ownership, destructive settings and user administration |
| admin | camera/provider/storage management |
| viewer | live view, permitted playback and export |

Provider and camera secrets are never returned by GET endpoints. An update payload may contain a replacement secret; an omitted secret means keep the existing value.

Browser sessions use an HttpOnly `camrelay_session` cookie. API clients may send
the bearer token returned by login or token creation. The refresh endpoint rotates
server-side session tokens; logout revokes the SQLite session and clears the cookie.

When `live_enabled` is true and a camera relay is running, the live-ticket
endpoint starts the configured media gateway and returns a 15-minute,
camera-scoped ticket envelope. The current gateway protocol is `hls`; clients
must branch on `protocol` and show a clear unsupported-state for adapters they
do not implement. FFmpeg reads a loopback RTSP credential proxy; camera
credentials do not appear in the FFmpeg command line or leave camrelay for a
sidecar service.

Provider names are immutable after creation because camera records reference the
provider by name. A provider in use by a camera cannot be deleted.

The current camera POST writes SQLite when database mode is enabled and writes
legacy JSON otherwise. It returns only a secret-free summary. Camera startup,
provider lookup, auto-start, recording reconciliation, token management, and
the v1 management surface use the same SQLite source when that mode is enabled;
legacy `/api` handlers remain only for rollback.

Audit history records authenticated mutation method, path, response status,
actor, and server timestamp. It intentionally excludes request bodies, query
strings, camera credentials, provider secrets, and token values. Reading audit
history requires an owner or admin role and SQLite mode.
