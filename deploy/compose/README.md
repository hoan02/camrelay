# Camrelay compose deployment

1. Copy `.env.example` to `.env` and set `CAMRELAY_SECRET_KEY`.
2. Copy the sanitized examples from the repository root into `data/` as
   `config.json`, `brands.json`, `cameras.json`, and `tokens.json`.
3. Keep `database_enabled` false for the first boot, verify the relay, then
   enable it only after backing up the secret key.
4. Start the service:

   ```bash
   docker compose up -d --build
   ```

The image includes FFmpeg and rclone for the recording/archive path. It runs
as the unprivileged `camrelay` user. Keep port 8080 behind a TLS reverse proxy
when it is reachable outside the trusted LAN.

The image contains both web builds. Set `web_root` to
`/opt/camrelay/apps/web/dist` in `data/config.json` only after the React
console has passed feature-parity checks; leave it as `static` to use the
legacy console.

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
