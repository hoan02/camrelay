# Authorized device validation matrix

Camrelay must not infer brand compatibility from a provider label. Record
results per model, firmware, region, app/account path, and provider profile.
Use only devices and accounts that the operator is authorized to administer.

## Matrix

| Brand / model | Firmware | Region | Provider profile | Source evidence | Tunnel | RTSP auth | HLS | Recording | Reconnect | Result |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| IMOU / TBD | TBD | TBD | TBD | Not tested | — | — | — | — | — | Unclaimed |
| Dahua / TBD | TBD | TBD | TBD | Not tested | — | — | — | — | — | Unclaimed |
| KBVision / TBD | TBD | TBD | TBD | Not tested in this repo | — | — | — | — | — | Unclaimed |

Do not replace `Unclaimed` with `Compatible` until the complete path has been
tested and the evidence is attached to the row.

## Test sequence

1. Record model, firmware, region, app version, provider profile name, and the
   authorization basis. Do not commit passwords, app keys, tokens, or private
   account identifiers.
2. Run the provider probe. This proves only signaling endpoint reachability and
   platform credentials; it does not prove camera compatibility.
3. Start one tunnel and capture the diagnostics result, tunnel status, and
   reconnect behavior.
4. Test the exact local RTSP path with `ffplay -rtsp_transport tcp` using the
   camera credential, then test the same camera through ticketed HLS.
5. Enable recording for at least one closed segment. Verify playback, archive
   behavior, and a restart with the same configuration.
6. Repeat after a camera offline/online transition and record whether the
   tunnel, HLS worker, recorder, and client recover independently.

## Evidence format

For each tested row, attach a redacted note containing:

- timestamp and Camrelay commit;
- model, firmware, region, provider profile, and camera channel/path;
- health, diagnostics, tunnel, RTSP, HLS, recording, archive, and reconnect
  observations;
- exact failure stage and safe next action when the result is not successful.

Until a row has this evidence, the README and UI must describe the provider as
configured or experimental, never as a compatibility guarantee.
