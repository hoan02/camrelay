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
tickets, the credential-free, protocol-aware live-ticket contract (HLS is the
current adapter), and a ticket-backed
`video_player` surface for live and recorded playback. Use `resolveUrl()` only
to turn the API's relative ticket URL into the appliance URL; the bearer token
remains in secure storage and is never appended to the media URL.

## Run

Install the Flutter SDK, then from this directory:

```bash
flutter pub get
flutter analyze
flutter test
flutter build apk --debug
flutter run --dart-define=CAMRELAY_API_BASE_URL=https://camrelay.example.com
```

On Windows, if Flutter was installed without adding it to the user PATH, open
a new PowerShell window and run:

```powershell
$flutterBin = Join-Path $env:USERPROFILE "develop\flutter\bin"
$env:Path = "$flutterBin;$env:Path"
flutter doctor
```

The Android SDK and Flutter SDK are separate installations. If `flutter doctor`
reports unknown Android licenses, review and accept them with
`flutter doctor --android-licenses` before building a release artifact.

For a local appliance on the same LAN, use its HTTPS reverse-proxy address.
Avoid shipping an HTTP endpoint or putting an access token in a URL, log, or
deep link.

The Android runner is included in this repository. The debug APK is written to
`build/app/outputs/flutter-apk/app-debug.apk`; install it on an authorized USB
device with `adb install -r build/app/outputs/flutter-apk/app-debug.apk` after
enabling USB debugging. iOS project generation and device builds require macOS
and Xcode.

## Current limitations

- the Android source/runner and debug APK build are verified locally, but live
  HLS playback still needs an authorized Android device and running appliance;
- an iOS runner and device build are not verified on Windows;
- WebRTC is an experimental backend adapter behind the private MediaMTX
  profile; this Flutter player intentionally keeps unknown protocols out of
  the HLS/video_player path until a real-device adapter is verified;
- playback behavior against a real camera still needs device validation;
- the activity feed currently represents persisted recording segments; motion events
  need a camera/provider event source and are not claimed by this client yet.
