# Camrelay compose deployment

1. Copy `.env.example` to `.env` and set `CAMRELAY_SECRET_KEY`.
2. Copy the sanitized examples from the repository root into `data/` as
   `config.json`, `brands.json`, `cameras.json`, and `tokens.json`.
3. Keep `database_enabled` false for the first boot, verify the relay, then
   enable it only after backing up the secret key.
4. Preview the legacy JSON migration before enabling SQLite:

   ```powershell
   ./migration-preview.ps1 -DataDir ./data
   ```

   The preview reports counts, duplicate IDs/ports, missing provider
   references, parse errors, and whether `CAMRELAY_SECRET_KEY` is present. It
   never prints credential values or changes the data directory.
5. Start the service:

   ```bash
   docker compose up -d --build
   ```

## Private WebRTC profile

WebRTC is an explicit opt-in. The override keeps RTSP (`8554`), MediaMTX
WHEP signaling (`8889`), and ICE UDP (`8189`) on an internal Docker network.
Camrelay publishes to the fixed internal destination
`rtsp://mediamtx:8554/camrelay/<camera-id>` without credentials, then proxies
WHEP through a short-lived Camrelay live ticket.

```bash
docker compose \
  -f docker-compose.yml \
  -f docker-compose.webrtc.yml \
  up -d --build
```

The baseline intentionally publishes no MediaMTX port to the host. A reviewed
LAN ICE gateway or TURN deployment can be added later after firewall and
remote-network requirements are known. Do not publish RTSP or WHEP directly,
and do not add credentials to an RTSP URL.

This profile is a signaling/media-gateway foundation. Browser/device playback
still depends on camera codec support and an authorized-camera test; TURN for
remote networks and broad codec transcoding remain roadmap items.

The image includes FFmpeg and rclone for the recording/archive path. It runs
as the unprivileged `camrelay` user. Keep port 8080 behind a TLS reverse proxy
when it is reachable outside the trusted LAN.

The image contains both web builds. The Compose environment defaults
`CAMRELAY_WEB_ROOT` to `/opt/camrelay/apps/web/dist`, so the React console is
served by default without rewriting the mounted `data/config.json`. Set
`CAMRELAY_WEB_ROOT=/opt/camrelay/static` in `.env` to use the legacy rollback
console.

## Backup and restore

Stop the service before taking a filesystem backup so the SQLite file and
recording index are consistent:

```powershell
docker compose stop camrelay
./backup.ps1
docker compose start camrelay
```

The script writes a timestamped directory under `backups/` with a SHA-256
manifest. It does not copy `CAMRELAY_SECRET_KEY` or the rclone credential;
store those separately in the deployment secret store. To restore, stop the
service and run:

```powershell
docker compose stop camrelay
./restore.ps1 -BackupPath ./backups/YYYYMMDD-HHMMSS
docker compose start camrelay
```

Restore is intentionally explicit and overwrites files listed in the backup
manifest only.
