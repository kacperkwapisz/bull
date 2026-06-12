import { Hyper, ok, route } from "@hyper/core"
import { hyperLog } from "@hyper/log"
import { corsPlugin } from "@hyper/cors"
import { rateLimit } from "@hyper/rate-limit"
import { loadEnv, corsOrigins } from "./lib/env.ts"
import { authRoutes } from "./routes/auth.ts"
import { coachRoutes } from "./routes/coach.ts"
import { dataRoutes } from "./routes/data.ts"
import { ensureSchema, pingDb } from "./db/client.ts"

const env = loadEnv()

const health = route.get("/health").handle(async () =>
  ok({
    ok: true,
    service: "bull-api",
    upstream: env.BULL_UPSTREAM_BASE_URL,
    model_default: env.BULL_MODEL_DEFAULT,
    model_deep: env.BULL_MODEL_DEEP,
    persistence: env.DATABASE_URL ? (await pingDb(env)) : false,
  }),
)

const app = new Hyper()
  .use(hyperLog({ service: "bull-api" }))
  .use(
    corsPlugin({
      origin: corsOrigins(env) === "*" ? "*" : corsOrigins(env),
      credentials: corsOrigins(env) !== "*",
      allowAnyOrigin: corsOrigins(env) === "*",
    }),
  )
  .use(rateLimit({ window: "1m", limit: 120 }))
  .use(health)
  .use(authRoutes(env))
  .use(coachRoutes(env))
  .use(dataRoutes(env))

export default app

// Migrations are applied out-of-band by the Docker entrypoint (`bun run
// db:migrate`) before the server starts, so the app never mutates schema at
// request time. Locally, run `bun run db:migrate` once against your DATABASE_URL.
if (process.env.HYPER_SKIP_LISTEN !== "1") {
  // Bind to all interfaces by default so on-device clients can reach the API
  // over the local/Tailscale network (Bun's "localhost" default resolves to the
  // IPv6 loopback ::1, which is unreachable from a phone). Override with HOST.
  app.listen({ port: Number(env.PORT), hostname: process.env.HOST ?? "0.0.0.0" })
}