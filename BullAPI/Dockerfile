# Bull API — Bun + Hyper (production)
# Migrations run on boot via docker-entrypoint.sh (drizzle tracks applied
# migrations, so it's idempotent and safe across restarts/replicas).
FROM oven/bun:1.3.2-alpine AS base
WORKDIR /app

FROM base AS deps
COPY package.json bun.lock ./
RUN bun install --frozen-lockfile --production

FROM base AS release
ENV NODE_ENV=production
ENV PORT=3000
ENV HYPER_SKIP_LISTEN=1
# Raw uploaded bundles land here. Mount a volume to persist across restarts.
ENV BULL_BUNDLE_DIR=/app/bundles

COPY --from=deps /app/node_modules ./node_modules
COPY package.json bun.lock tsconfig.json hyper.config.json hyper.lock.json drizzle.config.ts ./
COPY src ./src
COPY docker-entrypoint.sh ./docker-entrypoint.sh

RUN chmod +x docker-entrypoint.sh \
  && mkdir -p "$BULL_BUNDLE_DIR" \
  && addgroup -g 1001 -S bull && adduser -S bull -u 1001 -G bull \
  && chown -R bull:bull /app
USER bull

EXPOSE 3000

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD bun -e "fetch('http://127.0.0.1:3000/health').then(async (r)=>{if(!r.ok)process.exit(1);const j=await r.json();process.exit(j.ok?0:1)}).catch(()=>process.exit(1))"

ENTRYPOINT ["./docker-entrypoint.sh"]
CMD ["bun", "run", "start"]
