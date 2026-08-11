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
| Foundation | workspace boundary, database, API v1, identity and observability |
| Media | gateway-backed WebRTC/HLS live view and durable recording pipeline |
| Console | React Console with onboarding, live wall, camera detail and timeline |
| Operations | archive, retention, diagnostics, backup/restore and audit history |
| Mobile | Flutter live/event/playback companion using the same API contract |

## UX principles

- Keep live video and timeline playback as separate tasks.
- Surface camera state with semantic status, never decorative color alone.
- Put protocol/provider information under Diagnostics, not normal setup.
- Every empty/error state names the next safe action.
- Share typography, status colors, spacing, motion and accessibility rules through design tokens.
