# Camrelay product roadmap

## User journeys

1. Owner installs Camrelay, creates an account, and configures a provider.
2. Owner adds one camera, verifies the tunnel, and starts local media.
3. Viewer opens a live camera without seeing RTSP or vendor credentials.
4. Viewer finds an event on a timeline and plays a recording.
5. Admin can distinguish provider, tunnel, media, recording, archive, and storage failures.

## Information architecture

### Web Console

- Overview: appliance health, camera state, storage, recent events.
- Cameras: live wall and camera list.
- Camera detail: Live, Timeline, Events, Diagnostics, Settings.
- Recordings: cross-camera timeline, filters, playback and export.
- Administration: providers, storage/archive, users, API/integrations, system.

### Mobile

- Home: favourite cameras and urgent events.
- Camera: live view and minimal controls.
- Activity: events and recordings.
- Account: server selection, notification and security preferences.

Mobile is a companion experience, not a compressed copy of administrative web forms.

## Phases

| Phase | Outcome |
| --- | --- |
| Foundation | **Complete for v1 management surface:** workspace boundary, SQLite, API v1, identity, RBAC, readiness and contract |
| Media | **In progress:** durable local recording/archive and ticketed local HLS live view exist; WebRTC and device validation remain |
| Console | **Management surface complete:** React onboarding, dashboard live wall, normalized recording activity, server-filtered recording timeline, camera/provider/token/recording/settings/diagnostics routes and camera live action; richer event markers remain |
| Operations | **In progress:** archive, diagnostics, backup/restore and privacy-safe audit history exist; retention remains |
| Mobile | **Android build verified:** Flutter client follows the same API contract and has ticket-backed HLS live/recorded playback plus normalized recording activity; real-device playback, iOS runner/device verification and WebRTC adapter remain |

## UX principles

- Keep live video and timeline playback as separate tasks.
- Surface camera state with semantic status, never decorative color alone.
- Put protocol/provider information under Diagnostics, not normal setup.
- Every empty/error state names the next safe action.
- Share typography, status colors, spacing, motion and accessibility rules through design tokens.
