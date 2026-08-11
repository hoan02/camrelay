# Camrelay Mobile

The mobile companion is a Flutter client for the versioned Camrelay API. It is
intentionally a companion experience, not a compressed copy of the
administrative web console.

## Scope

- sign in to a Camrelay appliance over HTTPS;
- keep the bearer token in platform secure storage;
- rotate an expiring bearer session once through the v1 refresh endpoint;
- show tunnel state and start/stop controls for authorized users;
- list recordings and request ticket-protected playback;
- show normalized recording/system activity from the v1 events endpoint;
- keep all P2P platform, camera RTSP, and archive credentials on the server.

The first scaffold uses Flutter Material 3 with the Camrelay signal-room color
language. The API client follows `docs/openapi-v1.yaml`; it does not call the
legacy `/api` surface. It includes camera lifecycle, recording playback
tickets, the credential-free HLS live-ticket contract, and a ticket-backed
`video_player` surface for live and recorded playback. Use `resolveUrl()` only
to turn the API's relative ticket URL into the appliance URL; the bearer token
remains in secure storage and is never appended to the media URL.

## Run

Install the Flutter SDK, then from this directory:

```bash
flutter pub get
flutter analyze
flutter test
flutter run --dart-define=CAMRELAY_API_BASE_URL=https://camrelay.example.com
```

For a local appliance on the same LAN, use its HTTPS reverse-proxy address.
Avoid shipping an HTTP endpoint or putting an access token in a URL, log, or
deep link.

## Current limitations

- HLS playback is wired through `video_player`; actual Android/iOS support
  still needs verification with a running Camrelay appliance;
- WebRTC remains a future adapter because the backend currently issues HLS
  tickets only;
- automatic refresh and media playback still need Flutter SDK/device verification on the development machine.
- the activity feed currently represents persisted recording segments; motion events
  need a camera/provider event source and are not claimed by this client yet.
