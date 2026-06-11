# Bull Coach API (Hyper + Bun)

Local-first Coach inference gateway for the Bull iOS app. Tools run on the device; this service streams model output from an OpenAI-compatible upstream (Groq, Fireworks, OpenCode Zen, etc.).

## Quick start

```bash
cd CoachAPI
cp .env.example .env
# Set COACH_UPSTREAM_API_KEY (Groq, Zen, …) and JWT_SECRET (32+ bytes)
bun run dev
```

Simulator Debug builds use `http://127.0.0.1:3000` by default (`CoachAPIConfiguration.swift`).

## Auth (v1 alpha)

- `POST /v1/auth/dev-token` — issues a JWT when `COACH_DEV_AUTH_BYPASS=1` (alpha / TestFlight dev).
- `POST /v1/coach/responses` — SSE stream (Responses-shaped events for iOS). Requires `Authorization: Bearer`.

## Upstream defaults

| Env | Default |
|-----|---------|
| `COACH_UPSTREAM_BASE_URL` | `https://api.groq.com/openai/v1` |
| `COACH_MODEL_DEFAULT` | `openai/gpt-oss-120b` |
| `COACH_MODEL_DEEP` | `openai/gpt-oss-120b` |

### OpenCode Zen (dev)

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

See `docs/bull-swift-mvp/CoachLLMEvalDecision.md` for manual smoke notes (no automated harness in v1).