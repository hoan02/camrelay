# v1 migration plan

## Preserve before changing

1. Keep the prototype-v0.1 tag as the known relay baseline.
2. Keep upstream attribution and the existing MIT license text.
3. Keep compatibility examples and authorized real-device test evidence.
4. Do not delete legacy JSON runtime files until import and rollback have been verified.

## JSON to SQLite

The first v1 startup detects legacy config.json, brands.json, cameras.json, tokens.json, and recordings.json.

- Read-only dry-run reports what will be imported.
- Import creates a timestamped backup next to the legacy files.
- IDs are preserved where possible.
- Passwords, platform keys, and tokens move to the secret/session model.
- A migration marker prevents duplicate imports.

## UI transition

1. Build React Console screens against v1 mock/OpenAPI client.
2. Enable the new console behind an explicit configuration flag.
3. Verify feature parity for onboarding, camera operations, providers, recordings, and settings.
4. Make the new console default.
5. Remove the legacy static UI only in a later release.

## Release gates

- P2P regression tests and at least one authorized device smoke test pass.
- Database migration can run twice without duplicate data.
- Access-control tests prove viewer/admin/owner boundaries.
- Web build and API-contract checks pass in CI.
- Health/readiness and backup/restore procedures are documented.
