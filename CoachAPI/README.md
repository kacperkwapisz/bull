# Bull Coach API (Hyper + Bun)

Local-first Coach inference gateway for the Bull iOS app. Tools run on the device; this service streams model output from an OpenAI-compatible upstream (oraiapi, Groq, OpenCode Zen, etc.).

## Quick start

```bash
cd CoachAPI
cp .env.example .env
# Set COACH_UPSTREAM_API_KEY (oraiapi dev-key, Groq, Zen, …) and JWT_SECRET (32+ bytes)
bun run dev
```

Simulator Debug builds use `http://100.95.172.121:3333` by default.

The shared Xcode scheme (`BullSwift.xcodeproj/xcshareddata/xcschemes/BullSwift.xcscheme`) automatically injects:
```
COACH_API_BASE_URL=http://100.95.172.121:3333
```
into the Run action for local development. Just select the "BullSwift" scheme and Run.

If you need a different address (other machine, different port, etc.):
- Edit the scheme in Xcode → Run → Arguments → Environment Variables, or
- Set `COACH_API_BASE_URL` in your own scheme / Info.plist, or
- The hardcoded fallback in `CoachAPIConfiguration.swift` will be used.

To run the Coach API server on a custom host/port:
```bash
PORT=3333 bun run dev
```

## Auth (v1 alpha)

- `POST /v1/auth/dev-token` — issues a JWT when `COACH_DEV_AUTH_BYPASS=1` (alpha / TestFlight dev).
- `POST /v1/coach/responses` — SSE stream (Responses-shaped events for iOS). Requires `Authorization: Bearer`.

## Upstream defaults

| Env | Default | Notes |
|-----|---------|-------|
| `COACH_UPSTREAM_BASE_URL` | `https://oraiapi.com/v1` | Same base used for both tiers. Current primary: oraiapi (OpenAI chat completions compatible, tools supported). |
| `COACH_MODEL_DEFAULT` | `gpt-oss-120b` | Backing model for `model_tier: "default"` (app "Coach" preset) |
| `COACH_MODEL_DEEP` | `gpt-oss-120b` | Backing model for `model_tier: "deep"` (app "Deeper insight" preset) |

### oraiapi (current default)

```env
COACH_UPSTREAM_BASE_URL=https://oraiapi.com/v1
COACH_UPSTREAM_API_KEY=dev-key
COACH_MODEL_DEFAULT=gpt-oss-120b
COACH_MODEL_DEEP=gpt-oss-120b
```

### OpenCode Zen (dev only)

```env
COACH_UPSTREAM_BASE_URL=https://opencode.ai/zen/v1
COACH_MODEL_DEFAULT=deepseek-v4-flash-free
```

## Docker (production)

```bash
docker build -t bull-coach-api CoachAPI
docker run --rm -p 3000:3000 \
  -e JWT_SECRET='your-32-byte-minimum-secret........' \
  -e COACH_UPSTREAM_API_KEY='...' \
  -e COACH_DEV_AUTH_BYPASS=0 \
  bull-coach-api
```

### GitHub Actions

| Workflow | Purpose |
|----------|---------|
| `coach-api-ci.yml` | PR/main: `bun test`, typecheck, Docker build + `/health` smoke |
| `coach-api-docker.yml` | Push to `main` or tag `coach-api-v*`: publish to **GHCR** |

Image: `ghcr.io/<owner>/coach-api:latest` (and `:sha`, branch tags). Set the package **public** or grant pull access for deploy hosts.

**Runtime env (required in prod):** `JWT_SECRET`, `COACH_UPSTREAM_API_KEY`. Optional: `COACH_UPSTREAM_BASE_URL`, `COACH_MODEL_*`, `CORS_ORIGINS`, `COACH_DEV_AUTH_BYPASS=0`.

## Tests

```bash
bun test
```

## Model choice

Coach supports two tiers chosen by the iOS client (persisted user preference, exposed in the chat sheet's profile menu under "Model"):

- `default` tier (`Coach` preset in app) — uses `COACH_MODEL_DEFAULT`
- `deep` tier (`Deeper insight` preset in app) — uses `COACH_MODEL_DEEP`

The client sends `model_tier` in the request body; the API resolves the concrete model string at request time. This keeps model selection server-controlled (no need to ship new app builds to change which LLM backs a tier).

Current defaults (both tiers):

- `gpt-oss-120b` on https://oraiapi.com/v1 (see `.env.example`). The oraiapi endpoint is OpenAI-compatible and supports tool calling.

See `docs/bull-swift-mvp/CoachLLMEvalDecision.md` for manual smoke notes and when to re-evaluate / swap the deep tier (no automated harness in v1). Re-run smoke checklist when changing either `COACH_MODEL_*` or the tier mapping.