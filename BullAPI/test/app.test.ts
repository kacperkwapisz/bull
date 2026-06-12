import { describe, expect, test } from "bun:test"

process.env.HYPER_SKIP_LISTEN = "1"
process.env.JWT_SECRET = "test-jwt-secret-at-least-32-bytes-long!!"
process.env.BULL_UPSTREAM_API_KEY = "test-upstream-key"

const app = (await import("../src/app.ts")).app

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

  test("POST /v1/auth/dev-token is gone (real accounts only)", async () => {
    const res = await app.fetch(
      new Request("http://localhost/v1/auth/dev-token", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ device_id: "test-device-12345678" }),
      }),
    )
    expect(res.status).toBe(404)
  })

  test("GET /v1/data/summary rejects requests without a session token", async () => {
    const res = await app.fetch(new Request("http://localhost/v1/data/summary"))
    expect(res.status).toBe(401)
  })
})