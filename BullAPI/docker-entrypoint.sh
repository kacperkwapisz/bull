#!/bin/sh
set -e

echo "🚀 Starting bull-api..."

if [ -n "$DATABASE_URL" ]; then
  echo "⏳ Running database migrations..."
  bun run db:migrate
else
  echo "⚠️  DATABASE_URL unset — skipping migrations (coach-only mode)."
fi

echo "🚀 Starting application..."
exec "$@"
