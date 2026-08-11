# Camrelay v1 architecture

## Product boundary

Camrelay is a self-hosted camera gateway and private NVR. The backend owns camera credentials, P2P sessions, media tickets, recordings, and archive access. Web and mobile clients never receive a device RTSP password or a P2P platform secret.

## Target shape

    Remote camera
      -> camrelay-core (P2P/PTCP and local RTSP)
      -> local RTSP credential proxy -> FFmpeg HLS (optional)
      -> camrelay-api (identity, policy, API, realtime events)
      -> camrelay-media (recording, archive, retention)
      -> web console / mobile app

v1 remains a modular monolith: one deployable Rust service plus purpose-built media sidecars. Crates are separated for ownership and testability, not deployed as microservices.

## Repository layout

    crates/
      camrelay-core/       P2P, PTCP, tunnel lifecycle and device capabilities
      camrelay-contract/   API DTOs, error codes and permission primitives
      camrelay-api/        Axum HTTP API, identity, persistence and realtime events
      camrelay-media/      FFmpeg, archive jobs and media-gateway integration
    apps/
      web/                 React Console
      mobile/              Flutter application
    packages/
      design-tokens/       Shared semantic design tokens for web and Flutter
    deploy/
      compose/             Local production stack
      caddy/               TLS reverse-proxy examples

## Runtime responsibilities

| Component | Owns | Must not own |
| --- | --- | --- |
| camrelay-core | P2P protocol, tunnel lifecycle, local RTSP endpoints | HTTP, database schema, UI concerns |
| camrelay-api | identity, RBAC, API, live/playback ticket issuing | vendor credential exposure |
| camrelay-media | recording jobs, archive queue, local HLS worker | user authorization decisions |
| RTSP credential proxy | loopback Basic/Digest auth boundary for local consumers | remote P2P signaling |
| web/mobile | user interaction and media playback | device secrets or direct RTSP access |

## Security and persistence

- The server is the only component allowed to read vendor and camera credentials.
- User passwords use Argon2id. Provider/device secrets use an encrypted envelope keyed by `CAMRELAY_SECRET_KEY`; the key is never stored in SQLite or the repository. Refresh sessions are server-side and revocable.
- Web uses secure HttpOnly session cookies. Mobile uses short-lived access tokens and refresh tokens in platform secure storage.
- A live/playback URL is a short-lived ticket scoped to one user and camera/recording.
- SQLite in WAL mode is the default single-appliance database. SQLx migrations are the schema truth.
- Runtime JSON files are legacy import sources, not the v1 system of record.

## Engineering rules

- No unbounded queues in tunnel, recording, or event paths.
- Every long-running task accepts cancellation and produces structured logs.
- API responses are versioned under /api/v1.
- OpenAPI is a release contract for web and mobile clients.
- The legacy static console remains as an explicit rollback surface after React
  feature parity; production Compose now selects React by default.

## Current migration checkpoint

The repository is intentionally in a dual-surface phase. The root `camrelay` package still owns the working relay binary and legacy JSON handlers, while SQLite mode owns the v1 camera/provider snapshots, auth sessions, onboarding, CRUD, tunnel startup, recording reconciliation, local HLS, API tokens, RBAC, and readiness contract. `camrelay-contract` and `apps/web` are compiled/tested independently so API and UI contracts can evolve without taking the known P2P/PTCP path offline. The React console now has v1 parity for the current management surface; WebRTC, durable motion events, and Flutter SDK verification remain later gates.
