# Media gateway design

This document freezes the boundary for the next media phase without claiming
that WebRTC is implemented. The shipped transport is ticketed HLS. WebRTC is
an additional adapter that must preserve the same credential and authorization
boundaries.

## Current path

```text
camera P2P/PTCP
    -> camrelay tunnel
    -> loopback RTSP credential proxy
    -> FFmpeg HLS worker
    -> camera-scoped HLS ticket
    -> web / mobile / Frigate / FFmpeg
```

The HLS worker receives an RTSP URL pointing at the loopback proxy. It never
receives the camera username/password and no sidecar is trusted with those
secrets.

`config.json` selects the gateway with `live_protocol`. Use `hls` for the
currently implemented adapter. An unsupported value is exposed in the
read-only media capability response and live-ticket requests fail with a
specific `live.protocol_unavailable` error; Camrelay does not silently fall
back to a different protocol.

## Gateway boundary

Any future WebRTC gateway must accept only a credential-free local source:

```text
rtsp://127.0.0.1:<proxy-port>/cam/realmonitor?channel=1&subtype=0
```

The gateway may own codec conversion, packetization, ICE and WebRTC session
state. It must not own provider credentials, camera credentials, user roles,
or the SQLite database. A sidecar deployment must use a private service
network and must not expose its control API publicly.

## Client contract

The existing live response is intentionally transport-shaped:

```json
{
  "protocol": "hls",
  "url": "/api/v1/live/<ticket>/index.m3u8",
  "expires_in_seconds": 900
}
```

Clients must branch on `protocol` and ignore fields they do not understand. The
current web and mobile clients support `hls` plus HTTP playback tickets; an
unknown protocol is surfaced as an unsupported adapter instead of being passed
to an HLS player by accident.
The HLS URL is a short-lived, camera-scoped media ticket; the API bearer is
not appended to it. A future WebRTC response may use the same envelope with a
WebRTC signaling URL and scoped ticket metadata, but it must be added to the
OpenAPI contract before implementation.

## WebRTC implementation gates

1. Choose the gateway implementation and verify its license, supported codecs,
   ICE/TURN behavior, and container/network model.
2. Add a backend adapter with explicit start/stop ownership and cancellation.
3. Add a protocol-aware ticket endpoint and OpenAPI schemas; keep HLS as the
   fallback when WebRTC is unavailable.
4. Verify browser and Android/iOS clients against a real camera, including
   reconnect, expired tickets, multiple viewers, and a camera disconnect.
5. Add metrics for gateway session count, negotiation failures, ICE failures,
   source stalls, and ticket expiry without logging credentials or ticket URLs.

WebRTC is not marked complete until all five gates have evidence. Until then,
HLS remains the supported live transport.
