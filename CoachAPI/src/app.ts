import { Hyper, ok, route } from "@hyper/core"
import { hyperLog } from "@hyper/log"
import { corsPlugin } from "@hyper/cors"
import { rateLimit } from "@hyper/rate-limit"
import { loadEnv, corsOrigins } from "./lib/env.ts"
import { authRoutes } from "./routes/auth.ts"
import { coachRoutes } from "./routes/coach.ts"

const env = loadEnv()

const health = route.get("/health").handle(() =>
  ok({
    ok: true,
    service: "bull-coach-api",
    upstream: env.COACH_UPSTREAM_BASE_URL,
    model_default: env.COACH_MODEL_DEFAULT,
    model_deep: env.COACH_MODEL_DEEP,
  }),
)

const app = new Hyper()
  .use(hyperLog({ service: "bull-coach-api" }))
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

export default app

if (process.env.HYPER_SKIP_LISTEN !== "1") {
  app.listen(Number(env.PORT))
}