import { Hyper, ok, route } from "@hyper/core"
import { hyperLog } from "@hyper/log"
import { corsPlugin } from "@hyper/cors"
import { rateLimit } from "@hyper/rate-limit"
import { loadEnv, corsOrigins } from "./lib/env.ts"
import { authRoutes } from "./routes/auth.ts"
import { coachRoutes } from "./routes/coach.ts"
import { dataRoutes } from "./routes/data.ts"
import { adminRoutes } from "./routes/admin.ts"
import { ensureSchema, getDb, pingDb } from "./db/client.ts"
import { getObjectStore } from "./lib/object-store.ts"
import { parseAllPending } from "./services/parse-bundle.ts"

const env = loadEnv()

const health = route.get("/health").handle(async () =>
  ok({
    ok: true,
    service: "bull-api",
    revision: env.GIT_SHA ?? "unknown",
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
  .use(adminRoutes(env))

// Named export only. A default export with a `fetch` method makes Bun
// auto-serve a SECOND server with the default 10s idleTimeout — that hidden
// server is the one that answered requests and severed coach SSE streams
// mid-response (surfacing on iOS as URLError -1017).
export { app }

// Migrations are applied out-of-band by the Docker entrypoint (`bun run
// db:migrate`) before the server starts, so the app never mutates schema at
// request time. Locally, run `bun run db:migrate` once against your DATABASE_URL.
if (process.env.HYPER_SKIP_LISTEN !== "1") {
  // Boot Bun.serve directly instead of `app.listen()`: hyper's listen() does
  // not forward `idleTimeout`, so Bun's 10s default would sever long-lived
  // coach SSE streams mid-response (truncated chunked body → iOS URLError
  // -1017). SSE heartbeats keep proxies alive; idleTimeout is the local belt.
  //
  // Bind to all interfaces by default so on-device clients can reach the API
  // over the local/Tailscale network (Bun's "localhost" default resolves to the
  // IPv6 loopback ::1, which is unreachable from a phone). Override with HOST.
  const server = Bun.serve({
    port: Number(env.PORT),
    hostname: process.env.HOST ?? "0.0.0.0",
    routes: app.routes,
    fetch: app.fetch,
    idleTimeout: 120, // seconds without socket activity; heartbeats arrive every 10s
  })
  console.log(`bull-api listening on http://${server.hostname}:${server.port}`)

  // Background parse drain: clear any pending-bundle backlog without waiting for
  // an upload to trigger a per-user sweep (e.g. after a large historical sync).
  // No-op unless the sidecar + DB + object store are all configured.
  if (env.BULL_CORE_BIN && env.BULL_CORE_DATA_DIR) {
    const db = getDb(env)
    const store = getObjectStore(env)
    if (db && store) {
      const config = { binaryPath: env.BULL_CORE_BIN, dataDir: env.BULL_CORE_DATA_DIR }
      let draining = false
      const timer = setInterval(async () => {
        if (draining) return
        draining = true
        try {
          const result = await parseAllPending(db, store, config)
          if (result.imported > 0) {
            console.log(
              `[parse] imported ${result.imported} bundle(s)` +
                (result.computedUsers.length > 0
                  ? `; compute for ${result.computedUsers.length} user(s)`
                  : "; compute skipped (debounced)"),
            )
          }
        } catch (error) {
          console.error("[parse] background drain failed", error)
        } finally {
          draining = false
        }
      }, 20_000)
      // Don't let the drain timer keep the process alive on its own.
      timer.unref?.()
    }
  }
}