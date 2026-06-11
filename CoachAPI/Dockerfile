# Bull Coach API — Bun + Hyper (production)
FROM oven/bun:1.3.2-alpine AS base
WORKDIR /app

FROM base AS deps
COPY package.json bun.lock ./
RUN bun install --frozen-lockfile --production

FROM base AS release
ENV NODE_ENV=production
ENV PORT=3000
ENV HYPER_SKIP_LISTEN=

COPY --from=deps /app/node_modules ./node_modules
COPY package.json bun.lock tsconfig.json hyper.config.json hyper.lock.json ./
COPY src ./src

RUN addgroup -g 1001 -S coach && adduser -S coach -u 1001 -G coach \
  && chown -R coach:coach /app
USER coach

EXPOSE 3000

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD bun -e "fetch('http://127.0.0.1:3000/health').then(async (r)=>{if(!r.ok)process.exit(1);const j=await r.json();process.exit(j.ok?0:1)}).catch(()=>process.exit(1))"

CMD ["bun", "run", "start"]