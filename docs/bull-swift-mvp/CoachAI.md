# Bull Coach AI architecture

## Overview

- **iOS:** Deterministic Coach tab + chat (`CoachChatModel`). Read-only tools via `CoachToolRegistry` / `CoachLocalToolContext`.
- **CoachAPI:** Hyper (Bun) service in-repo at `BullAPI/`. Proxies OpenAI-compatible chat completions and maps SSE to Responses-style events for the existing iOS stream parser.
- **No Codex:** ChatGPT device OAuth and `backend-api/codex/responses` removed.

## Data flow

1. User sends message → iOS calls `POST /v1/coach/responses` with JWT + message list.
2. CoachAPI streams from upstream with tools enabled (round 1).
3. iOS executes tools locally, sends tool summary in round 2 (messages only — tools executed on device).
4. Assistant text streams to UI.

## Auth (alpha)

Session JWT via Sign in with Apple (`POST /v1/auth/apple`); real accounts only. Token in Keychain (`com.bull.swift.coach`). Consent gate: `CoachConsentStore`.

## Configuration

| iOS | `COACH_API_BASE_URL` via shared Xcode scheme (or env/Info.plist); Debug → `http://100.95.172.121:3333` (local dev) |
| API | `.env` — see `BullAPI/.env.example` |

## Models

Two client-selectable tiers (via chat profile menu "Model"):

- `coach` → `COACH_MODEL_DEFAULT`
- `deeperInsight` → `COACH_MODEL_DEEP`

Default upstream: **gpt-oss-120b** on https://oraiapi.com/v1 for both (configurable independently via env; oraiapi is OpenAI-compatible and supports tools). Server resolves tier → model id; client never hardcodes provider models. See `CoachLLMEvalDecision.md` and `BullAPI/README.md`. Alternative dev upstreams (e.g. OpenCode Zen) remain possible via env.