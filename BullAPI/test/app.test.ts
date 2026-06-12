import { describe, expect, test } from "bun:test"

process.env.HYPER_SKIP_LISTEN = "1"
process.env.JWT_SECRET = "test-jwt-secret-at-least-32-bytes-long!!"
process.env.BULL_UPSTREAM_API_KEY = "test-upstream-key"
process.env.BULL_DEV_AUTH_BYPASS = "1"

const app = (await import("../src/app.ts")).default

describe("BullAPI", () => {
  test("GET /health returns ok", async () => {
    const res = await app.fetch(new Request("http://localhost/health"))
    expect(res.status).toBe(200)
    const json = (await res.json()) as { ok: boolean; service: string; model_default?: string; model_deep?: string }
    expect(json.ok).toBe(true)
    expect(json.service).toBe("bull-api")
    expect(typeof json.model_default).toBe("string")
    expect(typeof json.model_deep).toBe("string")
  })

  test("POST /v1/auth/dev-token issues bearer token", async () => {
    const res = await app.fetch(
      new Request("http://localhost/v1/auth/dev-token", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ device_id: "test-device-12345678" }),
      }),
    )
    expect(res.status).toBe(201)
    const json = (await res.json()) as { access_token: string; coach_entitled: boolean }
    expect(json.access_token.length).toBeGreaterThan(20)
    expect(json.coach_entitled).toBe(true)
  })
})