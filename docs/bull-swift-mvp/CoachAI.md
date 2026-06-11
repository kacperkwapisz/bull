# Bull Coach AI architecture

## Overview

- **iOS:** Deterministic Coach tab + chat (`CoachChatModel`). Read-only tools via `CoachToolRegistry` / `CoachLocalToolContext`.
- **CoachAPI:** Hyper (Bun) service in-repo at `CoachAPI/`. Proxies OpenAI-compatible chat completions and maps SSE to Responses-style events for the existing iOS stream parser.
- **No Codex:** ChatGPT device OAuth and `backend-api/codex/responses` removed.

## Data flow

1. User sends message → iOS calls `POST /v1/coach/responses` with JWT + message list.
2. CoachAPI streams from upstream with tools enabled (round 1).
3. iOS executes tools locally, sends tool summary in round 2 (messages only — tools executed on device).
4. Assistant text streams to UI.

## Auth (alpha)

Dev JWT via `POST /v1/auth/dev-token` when `COACH_DEV_AUTH_BYPASS=1`. Token in Keychain (`com.bull.swift.coach`). Consent gate: `CoachConsentStore`.

## Configuration

| iOS | `COACH_API_BASE_URL` Info.plist or env; Debug → `127.0.0.1:3000` |
| API | `.env` — see `CoachAPI/.env.example` |

## Models

Default upstream: **gpt-oss-120b** (Groq). Optional Zen for local dev. See `CoachLLMEvalDecision.md`.