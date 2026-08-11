# v1 migration plan

## Preserve before changing

1. Keep the prototype-v0.1 tag as the known relay baseline.
2. Keep upstream attribution and the existing MIT license text.
3. Keep compatibility examples and authorized real-device test evidence.
4. Do not delete legacy JSON runtime files until import and rollback have been verified.

## JSON to SQLite

The first v1 startup detects legacy config.json, brands.json, cameras.json, and
tokens.json and imports them idempotently when SQLite mode is enabled. Provider,
camera, and token secrets require `CAMRELAY_SECRET_KEY` and are encrypted before
being stored. The migration marker prevents duplicate user/import work, while
upserts keep repeated legacy snapshots convergent.

The current startup path does not silently create a legacy-file backup or offer a
dry-run flag. Take an explicit deployment backup with
`deploy/compose/backup.ps1` (or an equivalent stopped-service filesystem
snapshot) before enabling SQLite. A future migration command will add a
read-only preview and automatic timestamped backup.

- Backup and restore are explicit deployment operations.
- IDs are preserved where possible.
- Passwords, platform keys, and tokens move to the secret/session model.
- A migration marker prevents duplicate imports.

## UI transition

1. Build React Console screens against the v1/OpenAPI client. **Complete.**
2. Enable the new console behind an explicit configuration flag. **Complete.**
3. Verify feature parity for onboarding, camera operations, providers, recordings, settings, tokens, and diagnostics. **Complete for the current management surface.**
4. Make the new console default in the production Compose profile. **In progress; `CAMRELAY_WEB_ROOT` now defaults to the React build.**
5. Remove the legacy static UI only in a later release after a rollback window and a real-device release smoke test.

## Release gates

- P2P regression tests and at least one authorized device smoke test pass.
- Database migration can run twice without duplicate data.
- Access-control tests prove viewer/admin/owner boundaries.
- Web build and API-contract checks pass in CI.
- Health/readiness and backup/restore procedures are documented.
