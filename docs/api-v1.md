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
    GET    /api/v1/cameras                authenticated, secret-free summaries
    GET    /api/v1/providers               authenticated, secret-free summaries
    GET    /api/v1/recordings              authenticated, secret-free summaries

The endpoints below are the target contract and remain pending until their
storage and authorization behavior is migrated from the legacy handlers:

    POST   /api/v1/auth/login
    POST   /api/v1/auth/refresh
    POST   /api/v1/auth/logout
    GET    /api/v1/me

    GET    /api/v1/cameras
    POST   /api/v1/cameras
    GET    /api/v1/cameras/{camera_id}
    PATCH  /api/v1/cameras/{camera_id}
    POST   /api/v1/cameras/{camera_id}/start
    POST   /api/v1/cameras/{camera_id}/stop
    GET    /api/v1/cameras/{camera_id}/diagnostics

    POST   /api/v1/cameras/{camera_id}/live-ticket
    GET    /api/v1/recordings
    POST   /api/v1/recordings/{recording_id}/playback-ticket
    POST   /api/v1/recordings/{recording_id}/archive

    GET    /api/v1/events
    GET    /api/v1/system/health
    GET    /api/v1/system/readiness
    GET    /api/v1/system/stream

## Roles

| Role | Scope |
| --- | --- |
| owner | appliance ownership, destructive settings and user administration |
| admin | camera/provider/storage management |
| viewer | live view, permitted playback and export |

Provider and camera secrets are never returned by GET endpoints. An update payload may contain a replacement secret; an omitted secret means keep the existing value.
