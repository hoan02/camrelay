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
