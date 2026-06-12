import { describe, expect, test } from "bun:test"
import type { Env } from "../src/lib/env.ts"
import { AppleAuthError, verifyAppleIdentityToken } from "../src/lib/apple-auth.ts"

const env = {
  APPLE_ISSUER: "https://appleid.apple.com",
  APPLE_BUNDLE_ID: "com.bull.swift",
} as unknown as Env

describe("verifyAppleIdentityToken", () => {
  test("returns identity from a verified payload", async () => {
    const identity = await verifyAppleIdentityToken(env, "fake.token.here", async () => ({
      payload: { sub: "apple-user-123", email: "x@privaterelay.appleid.com", is_private_email: "true" },
    }))
    expect(identity.sub).toBe("apple-user-123")
    expect(identity.email).toBe("x@privaterelay.appleid.com")
    expect(identity.isPrivateEmail).toBe(true)
  })

  test("maps verification failure to AppleAuthError", async () => {
    const err = await verifyAppleIdentityToken(env, "bad", async () => {
      throw new Error("bad signature")
    }).catch((e) => e)
    expect(err).toBeInstanceOf(AppleAuthError)
    expect((err as AppleAuthError).code).toBe("invalid_apple_token")
  })

  test("rejects a token with no subject", async () => {
    const err = await verifyAppleIdentityToken(env, "x", async () => ({ payload: {} })).catch(
      (e) => e,
    )
    expect(err).toBeInstanceOf(AppleAuthError)
    expect((err as AppleAuthError).code).toBe("missing_sub")
  })
})
