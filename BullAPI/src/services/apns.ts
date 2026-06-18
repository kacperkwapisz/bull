/**
 * APNs (Apple Push Notification service) sender.
 *
 * Token-based auth: a short-lived ES256 provider JWT signed with the team's .p8
 * key, sent over HTTP/2 to Apple. Everything is env-gated — when the APNs
 * settings are absent, `sendApnsAlert` is a no-op so the API runs without push
 * configured (mirrors the object-store / sidecar pattern).
 */

import http2 from "node:http2"
import { createPrivateKey, sign as cryptoSign } from "node:crypto"
import { and, eq } from "drizzle-orm"
import type { Db } from "../db/client.ts"
import { pushLog, pushTokens } from "../db/schema.ts"
import type { Env } from "../lib/env.ts"
import { hasApns } from "../lib/env.ts"

const APNS_HOST_PRODUCTION = "https://api.push.apple.com"
const APNS_HOST_SANDBOX = "https://api.sandbox.push.apple.com"
const PROVIDER_TOKEN_TTL_SECONDS = 50 * 60

function base64url(input: Buffer | string): string {
  return Buffer.from(input)
    .toString("base64")
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/, "")
}

let cachedProviderToken: { jwt: string; iat: number } | null = null

/** Signed ES256 provider JWT (cached ~50 min, well under APNs' 60-min limit). */
function providerToken(env: Env): string {
  const now = Math.floor(Date.now() / 1000)
  if (cachedProviderToken && now - cachedProviderToken.iat < PROVIDER_TOKEN_TTL_SECONDS) {
    return cachedProviderToken.jwt
  }
  // Allow the .p8 PEM to arrive with literal "\n" sequences (common in env vars).
  const pem = env.APNS_KEY_P8!.replace(/\\n/g, "\n")
  const header = base64url(JSON.stringify({ alg: "ES256", kid: env.APNS_KEY_ID! }))
  const payload = base64url(JSON.stringify({ iss: env.APNS_TEAM_ID!, iat: now }))
  const signingInput = `${header}.${payload}`
  // ES256 JWTs require the raw r||s signature (JOSE), not DER — `ieee-p1363`.
  const signature = cryptoSign("sha256", Buffer.from(signingInput), {
    key: createPrivateKey(pem),
    dsaEncoding: "ieee-p1363",
  })
  const jwt = `${signingInput}.${base64url(signature)}`
  cachedProviderToken = { jwt, iat: now }
  return jwt
}

export interface ApnsAlert {
  token: string
  /** "sandbox" (debug builds) or "production". */
  environment: string
  title: string
  body: string
  topic: string
  payload?: Record<string, unknown>
}

export interface ApnsResult {
  ok: boolean
  status: number
  reason?: string
}

/** Send one alert push. No-op (ok:false) when APNs is not configured. */
export async function sendApnsAlert(env: Env, alert: ApnsAlert): Promise<ApnsResult> {
  if (!hasApns(env)) return { ok: false, status: 0, reason: "apns_unconfigured" }
  const host = alert.environment === "sandbox" ? APNS_HOST_SANDBOX : APNS_HOST_PRODUCTION
  const jwt = providerToken(env)
  const body = JSON.stringify({
    aps: { alert: { title: alert.title, body: alert.body }, sound: "default" },
    ...(alert.payload ?? {}),
  })
  return await new Promise<ApnsResult>((resolve) => {
    const client = http2.connect(host)
    let settled = false
    const finish = (result: ApnsResult) => {
      if (settled) return
      settled = true
      client.close()
      resolve(result)
    }
    client.on("error", (error: Error) => finish({ ok: false, status: 0, reason: error.message }))
    const req = client.request({
      ":method": "POST",
      ":path": `/3/device/${alert.token}`,
      authorization: `bearer ${jwt}`,
      "apns-topic": alert.topic,
      "apns-push-type": "alert",
      "apns-priority": "10",
      "content-type": "application/json",
    })
    let status = 0
    let data = ""
    req.on("response", (headers) => {
      status = Number(headers[":status"]) || 0
    })
    req.setEncoding("utf8")
    req.on("data", (chunk: string) => {
      data += chunk
    })
    req.on("end", () => {
      if (status === 200) {
        finish({ ok: true, status })
        return
      }
      let reason: string | undefined
      try {
        reason = (JSON.parse(data) as { reason?: string }).reason
      } catch {
        reason = undefined
      }
      finish(reason === undefined ? { ok: false, status } : { ok: false, status, reason })
    })
    req.on("error", (error: Error) => finish({ ok: false, status: 0, reason: error.message }))
    req.setTimeout(10_000, () => finish({ ok: false, status: 0, reason: "timeout" }))
    req.end(body)
  })
}

function recoveryBody(score: number): string {
  if (score >= 67) return "You're primed — your body is ready to take on strain today."
  if (score >= 34) return "Moderate recovery — train with awareness and don't overreach."
  return "Low recovery — prioritize rest and keep today's strain gentle."
}

/**
 * Send the morning recovery alert for `day` to all of a user's devices, exactly
 * once per (user, day). De-dup is claimed up front via `push_log`, so a re-parse
 * of the same drain cycle never re-notifies. Tokens APNs reports as gone
 * (410 / BadDeviceToken / Unregistered) are pruned.
 */
export async function sendRecoveryPush(
  db: Db,
  env: Env,
  userId: string,
  day: string,
  score: number | null,
): Promise<void> {
  if (!hasApns(env) || score == null) return
  const claimed = await db
    .insert(pushLog)
    .values({ userId, kind: "recovery", dedupeKey: day })
    .onConflictDoNothing()
    .returning({ id: pushLog.id })
  if (claimed.length === 0) return // already sent for this user+day
  const tokens = await db.select().from(pushTokens).where(eq(pushTokens.userId, userId))
  if (tokens.length === 0) return
  const rounded = Math.round(score)
  const title = `Recovery ${rounded}%`
  const body = recoveryBody(rounded)
  for (const token of tokens) {
    const result = await sendApnsAlert(env, {
      token: token.token,
      environment: token.environment,
      title,
      body,
      topic: env.APNS_TOPIC,
      payload: { kind: "recovery", day },
    })
    const gone =
      result.status === 410 ||
      result.reason === "BadDeviceToken" ||
      result.reason === "Unregistered"
    if (gone) {
      await db
        .delete(pushTokens)
        .where(and(eq(pushTokens.userId, userId), eq(pushTokens.id, token.id)))
    }
  }
}
