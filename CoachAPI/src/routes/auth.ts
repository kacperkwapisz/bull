import { Hyper, created, ok, route } from "@hyper/core"
import { z } from "zod"
import type { Env } from "../lib/env.ts"
import { signCoachJwt } from "../lib/jwt-sign.ts"

const devTokenBody = z.object({
  device_id: z.string().min(8).max(128).optional(),
})

export function authRoutes(env: Env) {
  const devToken = route
    .post("/v1/auth/dev-token")
    .body(devTokenBody)
    .handle(async ({ body }) => {
      if (!env.COACH_DEV_AUTH_BYPASS) {
        return new Response(JSON.stringify({ error: "dev_auth_disabled" }), {
          status: 403,
          headers: { "content-type": "application/json" },
        })
      }
      const sub = body.device_id ?? `bull-dev-${crypto.randomUUID()}`
      const token = await signCoachJwt(env.JWT_SECRET, { sub, scope: "coach" })
      return created({
        access_token: token,
        token_type: "Bearer",
        expires_in: 60 * 60 * 24 * 30,
        coach_entitled: true,
      })
    })

  const entitlement = route.get("/v1/coach/entitlement").handle(() =>
    ok({
      coach_entitled: true,
      auth_mode: env.COACH_DEV_AUTH_BYPASS ? "dev_bypass" : "jwt",
    }),
  )

  return new Hyper({ prefix: "" }).use([devToken, entitlement])
}