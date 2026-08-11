# Camrelay Mobile

The mobile companion is a Flutter client for the versioned Camrelay API. It is
intentionally a companion experience, not a compressed copy of the
administrative web console.

## Scope

- sign in to a Camrelay appliance over HTTPS;
- keep the bearer token in platform secure storage;
- show camera state and start/stop controls for authorized users;
- list recordings and request ticket-protected playback;
- keep all P2P platform, camera RTSP, and archive credentials on the server.

The first scaffold uses Flutter Material 3 with the Camrelay signal-room color
language. The API client follows `docs/openapi-v1.yaml`; it does not call the
legacy `/api` surface.

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

- live browser/mobile media gateway integration is not wired yet;
- playback currently exposes the API ticket URL to the native video layer only
  after an explicit user action, and a dedicated video package will be chosen
  when the live-media contract is finalized;
- Flutter SDK verification is pending on the development machine.
