import { Hyper, created, ok, route } from "@hyper/core"
import { z } from "zod"
import type { Env } from "../lib/env.ts"
import { signCoachJwt } from "../lib/jwt-sign.ts"
import { getDb } from "../db/client.ts"
import { verifyAppleIdentityToken, AppleAuthError } from "../lib/apple-auth.ts"
import { upsertUserFromApple } from "../services/accounts.ts"

const appleBody = z.object({
  identity_token: z.string().min(20),
  device_id: z.string().min(8).max(128).optional(),
})

const SESSION_TTL_SECONDS = 60 * 60 * 24 * 30

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  })
}

export function authRoutes(env: Env) {
  // Sign in with Apple: verify the device-issued identity token, upsert the
  // Bull account, and return a user-scoped session token.
  const apple = route
    .post("/v1/auth/apple")
    .body(appleBody)
    .handle(async ({ body }) => {
      const db = getDb(env)
      if (!db) return json(503, { error: "persistence_unavailable" })
      let identity
      try {
        identity = await verifyAppleIdentityToken(env, body.identity_token)
      } catch (e) {
        if (e instanceof AppleAuthError) return json(401, { error: e.code, message: e.message })
        throw e
      }
      const account = await upsertUserFromApple(db, identity, body.device_id)
      const token = await signCoachJwt(env.JWT_SECRET, {
        sub: account.userId,
        scope: "user",
        userId: account.userId,
        ttlSeconds: SESSION_TTL_SECONDS,
      })
      return created({
        access_token: token,
        token_type: "Bearer",
        expires_in: SESSION_TTL_SECONDS,
        user_id: account.userId,
        is_new_user: account.created,
        coach_entitled: true,
      })
    })

  const entitlement = route.get("/v1/coach/entitlement").handle(() =>
    ok({
      coach_entitled: true,
      auth_mode: "jwt",
    }),
  )

  return new Hyper({ prefix: "" }).use([apple, entitlement])
}
