import { describe, expect, test } from "bun:test"

process.env.JWT_SECRET = "test-jwt-secret-at-least-32-bytes-long!!"

const { withSseHeartbeat } = await import("../src/routes/coach.ts")

async function collect(gen: AsyncGenerator<string | Uint8Array>): Promise<string[]> {
  const out: string[] = []
  for await (const chunk of gen) {
    out.push(typeof chunk === "string" ? chunk : new TextDecoder().decode(chunk))
  }
  return out
}

describe("withSseHeartbeat", () => {
  test("passes source chunks through unchanged when the source is fast", async () => {
    async function* fast(): AsyncGenerator<string | Uint8Array> {
      yield "data: a\n\n"
      yield "data: b\n\n"
    }
    const chunks = await collect(withSseHeartbeat(fast(), 1000))
    expect(chunks).toEqual(["data: a\n\n", "data: b\n\n"])
  })

  test("emits ping comments while the source is silent, then resumes", async () => {
    async function* slow(): AsyncGenerator<string | Uint8Array> {
      yield "data: first\n\n"
      await new Promise((resolve) => setTimeout(resolve, 120))
      yield "data: second\n\n"
    }
    const chunks = await collect(withSseHeartbeat(slow(), 30))
    expect(chunks[0]).toBe("data: first\n\n")
    expect(chunks.at(-1)).toBe("data: second\n\n")
    const pings = chunks.filter((c) => c === ": ping\n\n")
    expect(pings.length).toBeGreaterThanOrEqual(2)
  })

  test("propagates source errors", async () => {
    async function* failing(): AsyncGenerator<string | Uint8Array> {
      yield "data: ok\n\n"
      throw new Error("boom")
    }
    await expect(collect(withSseHeartbeat(failing(), 1000))).rejects.toThrow("boom")
  })
})
