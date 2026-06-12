/**
 * Sign in with Apple — identity token verification.
 *
 * The Bull app performs the native Apple sign-in and posts the resulting
 * identity token (a JWT issued by Apple) here. BullAPI verifies it against
 * Apple's public JWKS and enforces issuer + audience (our bundle id) before
 * minting a Bull session token. No Apple client secret is required for this
 * verification path.
 */

import { createRemoteJWKSet, jwtVerify, type JWTPayload } from "jose"
import type { Env } from "./env.ts"

export interface AppleIdentity {
  /** Apple's stable per-app subject identifier. */
  readonly sub: string
  readonly email?: string
  readonly isPrivateEmail: boolean
}

export class AppleAuthError extends Error {
  constructor(
    readonly code: string,
    message: string,
  ) {
    super(message)
  }
}

type JwksResolver = ReturnType<typeof createRemoteJWKSet>

let cachedJwks: JwksResolver | null = null
let cachedIssuer: string | null = null

function jwksFor(env: Env): JwksResolver {
  if (!cachedJwks || cachedIssuer !== env.APPLE_ISSUER) {
    cachedJwks = createRemoteJWKSet(new URL(`${env.APPLE_ISSUER}/auth/keys`))
    cachedIssuer = env.APPLE_ISSUER
  }
  return cachedJwks
}

/**
 * Verify an Apple identity token. `verifier` is injectable so tests can supply
 * a local key set without hitting Apple.
 */
export async function verifyAppleIdentityToken(
  env: Env,
  identityToken: string,
  verifier: (token: string) => Promise<{ payload: JWTPayload }> = (token) =>
    jwtVerify(token, jwksFor(env), {
      issuer: env.APPLE_ISSUER,
      audience: env.APPLE_BUNDLE_ID,
    }),
): Promise<AppleIdentity> {
  let payload: JWTPayload
  try {
    ;({ payload } = await verifier(identityToken))
  } catch (e) {
    throw new AppleAuthError("invalid_apple_token", e instanceof Error ? e.message : String(e))
  }
  if (typeof payload.sub !== "string" || payload.sub.length === 0) {
    throw new AppleAuthError("missing_sub", "Apple token has no subject")
  }
  const email = typeof payload.email === "string" ? payload.email : undefined
  const isPrivateEmail =
    payload.is_private_email === true || payload.is_private_email === "true"
  return {
    sub: payload.sub,
    ...(email !== undefined ? { email } : {}),
    isPrivateEmail,
  }
}
