# Bull API — Bun + Hyper, with the bull-core parse sidecar (production)
#
# Built from the REPO ROOT context (see .github/workflows/bull-api-docker.yml:
# `context: .`, `file: BullAPI/Dockerfile`) so this image can compile the
# bull-bridge-serve sidecar from Rust/core. The API spawns that static binary as
# a subprocess (newline-delimited JSON over stdio) to parse uploaded bundles
# server-side with the exact same parser/algorithms the device uses.
#
# Migrations run on boot via docker-entrypoint.sh (drizzle tracks applied
# migrations, so it's idempotent and safe across restarts/replicas).

# --- Stage 1: build the bull-core sidecar as a static musl binary ---------
# rust:alpine is musl-native, so a plain release build is statically linked and
# runs on the musl-based bun image below. rusqlite "bundled" compiles SQLite via
# cc, so the build needs a C toolchain.
FROM rust:1-alpine AS rust
WORKDIR /build
RUN apk add --no-cache musl-dev gcc
COPY Rust/core ./core
WORKDIR /build/core
# Build with the `server` profile (release optimisations + panic=unwind) so the
# sidecar catches per-frame panics instead of aborting the whole process.
RUN cargo build --profile server --locked --bin bull-bridge-serve

# --- Bun base -------------------------------------------------------------
FROM oven/bun:1.3.2-alpine AS base
WORKDIR /app

FROM base AS deps
COPY BullAPI/package.json BullAPI/bun.lock ./
RUN bun install --frozen-lockfile --production

FROM base AS release
ENV NODE_ENV=production
ENV PORT=3000
# Commit the image was built from (passed by CI as --build-arg GIT_SHA=...).
# Surfaced on /health so a running deploy is identifiable.
ARG GIT_SHA=unknown
ENV GIT_SHA=$GIT_SHA
# Server-side parse sidecar: the API spawns this binary over stdio, and keeps
# per-user bull-core SQLite stores under the data dir. Mount a persistent volume
# at /app/data in production so parsed compute state survives restarts.
ENV BULL_CORE_BIN=/app/bin/bull-bridge-serve
ENV BULL_CORE_DATA_DIR=/app/data
# HYPER_SKIP_LISTEN must NOT be set here: src/app.ts boots its own Bun.serve
# (with the SSE-safe idleTimeout) only when this flag is absent. Tests set the
# flag themselves to import the app without listening.
# Raw upload bundles are stored in object storage (S3/R2) via the S3_* runtime
# env — no local volume required for those.

COPY --from=deps /app/node_modules ./node_modules
COPY BullAPI/package.json BullAPI/bun.lock BullAPI/tsconfig.json BullAPI/hyper.config.json BullAPI/hyper.lock.json BullAPI/drizzle.config.ts ./
COPY BullAPI/src ./src
COPY BullAPI/docker-entrypoint.sh ./docker-entrypoint.sh
COPY --from=rust /build/core/target/server/bull-bridge-serve /app/bin/bull-bridge-serve

RUN chmod +x docker-entrypoint.sh /app/bin/bull-bridge-serve \
  && addgroup -g 1001 -S bull && adduser -S bull -u 1001 -G bull \
  && mkdir -p /app/data \
  && chown -R bull:bull /app
USER bull

EXPOSE 3000
VOLUME ["/app/data"]

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD bun -e "fetch('http://127.0.0.1:3000/health').then(async (r)=>{if(!r.ok)process.exit(1);const j=await r.json();process.exit(j.ok?0:1)}).catch(()=>process.exit(1))"

ENTRYPOINT ["./docker-entrypoint.sh"]
CMD ["bun", "run", "start"]
