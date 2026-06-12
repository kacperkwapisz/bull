# Bull API (Bun + Hyper)

Backend for the Bull app. Three capability groups in one service:

1. **Auth** — Sign in with Apple (real accounts) plus a dev-token bypass for local builds.
2. **Data** — ingest of device-originated WHOOP 5 data and read APIs for the
   forthcoming web app and for debugging what the device produces.
3. **Coach** — the existing local-first inference gateway (streams model output
   from an OpenAI-compatible upstream; tools run on the device).

**Independence:** every physiological metric stored here is derived from the
connected device's own live sensor data, uploaded by the Bull app. Bull does not
ingest physiological data from third-party health stores. The raw upload bundle
is kept as the source of record; the curated tables are a re-derivable projection
of it, used for queries and for honest debugging of device output.

## Quick start

```bash
cd BullAPI
cp .env.example .env
# Set JWT_SECRET (32+ bytes), BULL_UPSTREAM_API_KEY, and DATABASE_URL.
bun install
bun run db:migrate     # apply schema to DATABASE_URL (no-op concept: idempotent)
bun run dev
```

Persistence is optional: with no `DATABASE_URL`, coach + dev-token still work and
the data/Apple routes return `503 persistence_unavailable`.

## Routes

| Method | Path | Auth | Purpose |
|--------|------|------|---------|
| GET  | `/health` | — | Liveness; reports upstream + DB connectivity. |
| POST | `/v1/auth/apple` | — | Verify Apple identity token, upsert user, issue session JWT. |
| POST | `/v1/auth/dev-token` | — (bypass only) | Dev/TestFlight JWT when `BULL_DEV_AUTH_BYPASS=1`. |
| GET  | `/v1/coach/entitlement` | — | Coach entitlement + auth mode. |
| POST | `/v1/coach/responses` | Bearer | SSE coach stream (Responses-shaped events). |
| POST | `/v1/data/uploads` | Bearer (user) | Upload an export bundle (+ optional summary). |
| GET  | `/v1/data/summary` | Bearer (user) | Counts + latest day per metric family. |
| GET  | `/v1/data/recovery` | Bearer (user) | Daily recovery rows (`from`,`to`,`limit`). |
| GET  | `/v1/data/sleep` | Bearer (user) | Daily sleep rows. |
| GET  | `/v1/data/spo2` | Bearer (user) | Recent SpO₂ samples. |
| GET  | `/v1/data/uploads` | Bearer (user) | Upload bundle history + parse status. |

"User" routes require a token issued by `/v1/auth/apple` (carries `user_id`).

## Sign in with Apple

The Bull app performs native Apple sign-in and posts the resulting identity
token. The server verifies it against Apple's public JWKS, enforcing issuer
(`APPLE_ISSUER`) and audience (`APPLE_BUNDLE_ID`), then upserts one Bull user per
Apple subject and returns a 30-day session JWT. No Apple client secret is needed
for this verification path.

```http
POST /v1/auth/apple
{ "identity_token": "<apple jwt>", "device_id": "<optional>" }
→ 201 { "access_token", "user_id", "is_new_user", "expires_in", ... }
```

## Data ingest

`POST /v1/data/uploads` is a multipart form:

| Field | Type | Notes |
|-------|------|-------|
| `bundle` | file | Raw device export bundle. Stored verbatim (source of record), deduped per user by SHA-256. |
| `summary` | JSON string | Optional curated metrics projected into queryable tables. |
| `device_id` | string | Optional. |

`summary` shape (all fields optional; every value originates from the device's
own sensors):

```json
{
  "timeframe": { "start": "ISO", "end": "ISO" },
  "recovery": [{ "day": "YYYY-MM-DD", "recovery_score": 71, "hrv_ms": 64, "resting_hr_bpm": 52 }],
  "sleep":    [{ "day": "YYYY-MM-DD", "sleep_score": 88, "total_sleep_minutes": 451, "rem_minutes": 95, "deep_minutes": 80 }],
  "spo2":     [{ "recorded_at": "ISO", "spo2": 96 }]
}
```

Raw bundles land under `BULL_BUNDLE_DIR` (default `./bundles`, `/app/bundles` in
Docker — mount a volume to persist). Re-uploading identical bytes is idempotent.

## Database

Postgres via Drizzle. Schema lives in `src/db/schema.ts`; migrations in
`src/db/migrations/` (generated with `bun run db:generate`). Apply with
`bun run db:migrate`. Tables: `users`, `apple_identities`, `devices`,
`upload_bundles`, `daily_recovery`, `daily_sleep`, `spo2_samples`.

## Environment

| Env | Required | Default | Notes |
|-----|----------|---------|-------|
| `JWT_SECRET` | yes | — | 32+ bytes. Signs session tokens. |
| `BULL_UPSTREAM_API_KEY` | yes | — | Coach upstream key. |
| `DATABASE_URL` | for data/auth | — | Postgres connection string. |
| `APPLE_BUNDLE_ID` | for Apple | `com.bull.swift` | Required audience in Apple tokens. |
| `APPLE_ISSUER` | no | `https://appleid.apple.com` | Override only in tests. |
| `BULL_DEV_AUTH_BYPASS` | no | `0` | `1` enables `/v1/auth/dev-token`. |
| `BULL_UPSTREAM_BASE_URL` | no | `https://oraiapi.com/v1` | Coach upstream base. |
| `BULL_MODEL_DEFAULT` / `BULL_MODEL_DEEP` | no | `gpt-oss-120b` | Coach tier models. |
| `BULL_BUNDLE_DIR` | no | `./bundles` | Raw bundle storage root. |
| `CORS_ORIGINS` | no | `*` | Comma-separated allowlist. |

## Docker (production)

```bash
docker build -t bull-api BullAPI
docker run --rm -p 3000:3000 \
  -e JWT_SECRET='your-32-byte-minimum-secret........' \
  -e BULL_UPSTREAM_API_KEY='...' \
  -e DATABASE_URL='postgres://user:pass@host:5432/bull' \
  -e APPLE_BUNDLE_ID='com.bull.swift' \
  -e BULL_DEV_AUTH_BYPASS=0 \
  -v bull-bundles:/app/bundles \
  bull-api
```

The entrypoint runs migrations (`bun run db:migrate`) before starting; Drizzle
tracks applied migrations, so it is idempotent across restarts/replicas.

### GitHub Actions

| Workflow | Purpose |
|----------|---------|
| `bull-api-ci.yml` | PR/main: typecheck, migrate + `bun test` against a Postgres service, Docker build + `/health` smoke. |
| `bull-api-docker.yml` | Push to `main` or tag `bull-api-v*`: publish to GHCR (`ghcr.io/<owner>/bull-api`). |

**Runtime secrets (set in your host, never baked into the image):** `JWT_SECRET`,
`BULL_UPSTREAM_API_KEY`, `DATABASE_URL`. Optional: `APPLE_BUNDLE_ID`,
`BULL_UPSTREAM_BASE_URL`, `BULL_MODEL_*`, `CORS_ORIGINS`.

## Tests

```bash
bun test                    # unit suite; Postgres tests skip without a DB
TEST_DATABASE_URL=postgres://bull:bull@localhost:5432/bull_test bun test   # full
```

## iOS client contract

The current Bull app talks to `/v1/auth/dev-token` and `/v1/coach/*`, which are
unchanged. The Apple sign-in + data-upload client work is a follow-up (see
`docs/bull-swift-mvp/`): post the Apple identity token to `/v1/auth/apple`, store
the returned session JWT, and `POST` export bundles to `/v1/data/uploads`.
