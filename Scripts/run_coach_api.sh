#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT/CoachAPI"
if [[ ! -f .env ]]; then
  echo "Copy CoachAPI/.env.example to CoachAPI/.env and set COACH_UPSTREAM_API_KEY" >&2
  exit 1
fi
exec bun run dev