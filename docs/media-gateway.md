# Media gateway design

This document freezes the media boundary for the private-network phase. HLS
is the verified live transport. WebRTC now has a guarded Rust WHEP proxy and
an explicit MediaMTX Compose profile, but it is not a claim of universal
browser/device playback.

## Current paths

Verified HLS path:

```text
camera P2P/PTCP
    -> camrelay tunnel
    -> loopback RTSP credential proxy
    -> FFmpeg HLS worker
    -> camera-scoped HLS ticket
    -> web / mobile / Frigate / FFmpeg
```

Opt-in WebRTC foundation:

```text
camera P2P/PTCP
    -> camrelay tunnel
    -> loopback RTSP credential proxy
    -> FFmpeg publisher
    -> rtsp://mediamtx:8554/camrelay/<camera-id>
    -> MediaMTX on an internal Docker network
    -> Camrelay WHEP proxy
    -> short-lived live ticket
```

The publisher URL is fixed and contains no username, password, bearer token,
or media secret. Camera credentials remain inside Camrelay's RTSP proxy. The
MediaMTX RTSP and WHEP ports are private service ports, not host ports, in the
baseline Compose profile.

## Gateway boundary

MediaMTX is a media sidecar, not an identity or camera-credential service. It
must not own provider credentials, camera credentials, user roles, SQLite, or
live-ticket issuance. Camrelay owns the ticket and stores the upstream WHEP
session `Location`; clients receive only a Camrelay URL containing an opaque
short-lived ticket and local session id.

The WHEP proxy accepts only SDP-sized bodies, uses a fixed internal
`http://mediamtx:8889` destination, validates MediaMTX `Location` values, and
rejects external or path-escaping locations. It forwards no API bearer token
to MediaMTX.

## Configuration

The default HLS config remains safe and unchanged:

```json
{
  "live_enabled": true,
  "live_protocol": "hls",
  "webrtc_enabled": false
}
```

The private WebRTC profile is explicit:

```bash
docker compose \
  -f deploy/compose/docker-compose.yml \
  -f deploy/compose/docker-compose.webrtc.yml \
  up -d --build
```

It enables `CAMRELAY_WEBRTC_ENABLED` and adds MediaMTX to an internal Docker
network. It intentionally publishes no RTSP (`8554`), WHEP (`8889`), or ICE
(`8189`) port. A future LAN ICE gateway or TURN edge must be reviewed and
enabled separately, with firewall scope and remote-network behavior documented.

## Client contract

The live-ticket envelope stays transport-shaped:

```json
{
  "protocol": "webrtc",
  "url": "/api/v1/live/<ticket>/webrtc",
  "expires_in_seconds": 900
}
```

The WHEP flow is:

```text
POST  /api/v1/live/<ticket>/webrtc
PATCH /api/v1/live/<ticket>/webrtc/<session>
DELETE /api/v1/live/<ticket>/webrtc/<session>
```

Clients must branch on `protocol` and never pass an RTSP URL to a browser or
mobile player. HLS remains the fallback when WebRTC is disabled or unavailable.

## Verification gates

1. Validate the MediaMTX image/config and the private service network.
2. Test WHEP negotiation with an authorized camera and H264-compatible source.
3. Add web and Android/iOS WebRTC adapters with expired-ticket, reconnect,
   multiple-viewer, and camera-disconnect tests.
4. Decide the LAN ICE or TURN boundary; publish only the minimum reviewed UDP
   surface, never RTSP or MediaMTX control/signaling ports directly.
5. Add metrics for session count, negotiation/ICE failures, source stalls, and
   ticket expiry without logging credentials, SDP, or ticket URLs.

Until these gates have evidence, WebRTC is experimental and HLS is the
supported live transport.
