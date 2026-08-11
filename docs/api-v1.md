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
    GET    /api/v1/recordings              authenticated, secret-free summaries
    GET    /api/v1/recordings/{recording_id} authenticated, secret-free detail
    POST   /api/v1/recordings/{recording_id}/playback-ticket
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
    GET    /api/v1/providers/{provider_id}
    PATCH  /api/v1/providers/{provider_id}
    DELETE /api/v1/providers/{provider_id}
    GET    /api/v1/events
    GET    /api/v1/system/health
    POST   /api/v1/auth/login
    POST   /api/v1/auth/refresh
    POST   /api/v1/auth/logout
    GET    /api/v1/me

The endpoints below remain pending until their media/event transport is
implemented and tested end to end:

    POST   /api/v1/cameras/{camera_id}/live-ticket
    GET    /api/v1/system/stream

## Roles

| Role | Scope |
| --- | --- |
| owner | appliance ownership, destructive settings and user administration |
| admin | camera/provider/storage management |
| viewer | live view, permitted playback and export |

Provider and camera secrets are never returned by GET endpoints. An update payload may contain a replacement secret; an omitted secret means keep the existing value.

Browser sessions use an HttpOnly `camrelay_session` cookie. API clients may send
the bearer token returned by login or token creation. The refresh endpoint rotates
session tokens; logout revokes the in-memory session and clears the cookie.

Provider names are immutable after creation because camera records reference the
provider by name. A provider in use by a camera cannot be deleted.

The current camera POST writes SQLite when database mode is enabled and writes
legacy JSON otherwise. It returns only a secret-free summary. Camera startup,
provider lookup, auto-start, recording reconciliation, token management, and
the v1 management surface use the same SQLite source when that mode is enabled;
legacy `/api` handlers remain only for rollback.
