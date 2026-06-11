/**
 * HS256 JWT signing for Coach dev tokens. Verification uses @hyper/auth-jwt.
 */

function base64url(input: Uint8Array | string): string {
  const bytes = typeof input === "string" ? new TextEncoder().encode(input) : input
  let binary = ""
  for (const b of bytes) {
    binary += String.fromCharCode(b)
  }
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "")
}

async function hmacSign(secret: string, data: string): Promise<string> {
  const key = await crypto.subtle.importKey(
    "raw",
    new TextEncoder().encode(secret),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  )
  const sig = await crypto.subtle.sign("HMAC", key, new TextEncoder().encode(data))
  return base64url(new Uint8Array(sig))
}

export async function signCoachJwt(
  secret: string,
  claims: { sub: string; scope?: string; ttlSeconds?: number },
): Promise<string> {
  const now = Math.floor(Date.now() / 1000)
  const ttl = claims.ttlSeconds ?? 60 * 60 * 24 * 30
  const header = { alg: "HS256", typ: "JWT" }
  const payload = {
    sub: claims.sub,
    scope: claims.scope ?? "coach",
    iat: now,
    exp: now + ttl,
  }
  const head = base64url(JSON.stringify(header))
  const body = base64url(JSON.stringify(payload))
  const signingInput = `${head}.${body}`
  const sig = await hmacSign(secret, signingInput)
  return `${signingInput}.${sig}`
}