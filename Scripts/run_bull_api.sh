#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT/BullAPI"
if [[ ! -f .env ]]; then
  echo "Copy BullAPI/.env.example to BullAPI/.env and set BULL_UPSTREAM_API_KEY (and DATABASE_URL for data/auth)" >&2
  exit 1
fi
# Bind to all interfaces so on-device clients can reach the API over the
# local/Tailscale network. Override by exporting HOST before running.
export HOST="${HOST:-0.0.0.0}"
exec bun run dev