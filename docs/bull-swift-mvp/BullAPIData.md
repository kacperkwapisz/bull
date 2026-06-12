# BullAPI — Accounts & Data Pipeline

Status: server-side shipped; iOS client integration is a follow-up.

## Why

Store device-originated WHOOP 5 data server-side so (a) a forthcoming web app can
read it and (b) we can inspect exactly what the device produces while debugging.
Every physiological value originates from the connected device's own live
sensors, uploaded by the Bull app — never imported from third-party health
stores. The raw upload bundle is the source of record; curated tables are a
re-derivable projection.

Storage split: Postgres holds accounts, curated metrics, and bundle metadata;
the raw bundle bytes live in S3-compatible object storage (Cloudflare R2).
Downloads are short-lived presigned URLs served directly by the bucket.

## Architecture

```
iPhone (BullSwift) ──► BullAPI ──► Postgres
  native Apple sign-in    verify Apple identity token (JWKS)
  upload export bundles   store raw bundle + project curated rows
  coach chat (unchanged)  issue per-user session JWT
Web app (future)   ──► BullAPI read endpoints (user session JWT)
```

## Server (done)

- `POST /v1/auth/apple` — verify Apple identity token (issuer + `APPLE_BUNDLE_ID`
  audience), upsert one user per Apple subject, return a 30-day session JWT
  carrying `user_id`.
- `POST /v1/data/uploads` — multipart: `bundle` (raw export, stored verbatim in
  R2, deduped by SHA-256), optional `summary` (curated metrics), optional
  `device_id`.
- `GET /v1/data/uploads/:id/download` — 15-minute presigned URL for the raw
  bundle (bytes served by the bucket, not the API).
- `GET /v1/data/{summary,recovery,sleep,spo2,uploads}` — per-user reads with
  honest empty states.
- Schema: `users`, `apple_identities`, `devices`, `upload_bundles`,
  `daily_recovery`, `daily_sleep`, `spo2_samples` (`BullAPI/src/db/schema.ts`).

See `BullAPI/README.md` for request/response shapes and env.

## iOS client follow-up (not yet wired)

The current app uses `/v1/auth/dev-token` + `/v1/coach/*` (unchanged, still
working). To add accounts + upload:

1. **Sign in with Apple** — add the capability/entitlement; use
   `ASAuthorizationAppleIDProvider`; POST the `identityToken` to `/v1/auth/apple`;
   store the returned session JWT in the Keychain (alongside the existing
   `CoachAuthKeychain`).
2. **Uploader** — build an export bundle via the Rust core
   (`export_raw_timeframe`, already exposed through the bridge), derive the
   `summary` JSON from the same store accessors the app UI uses, and
   `POST` both to `/v1/data/uploads` with the session JWT. Background-task
   friendly; idempotent (re-uploading identical bytes dedupes).

Because the Xcode project uses explicit file references (not synchronized
groups), new Swift files must be added to `BullSwift.xcodeproj/project.pbxproj`
and built in Xcode — do that in the follow-up, not blind.
