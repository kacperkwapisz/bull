import { Hyper, route, stream } from "@hyper/core"
import { authJwt } from "@hyper/auth-jwt"
import { z } from "zod"
import type { Env } from "../lib/env.ts"
import { streamUpstreamChat } from "../services/upstream-chat.ts"
import {
  createChatToResponsesMapper,
  formatResponsesSseLine,
} from "../services/sse-map-chat-to-responses.ts"

const toolCallSchema = z.object({
  id: z.string(),
  type: z.literal("function").optional(),
  function: z.object({
    name: z.string(),
    arguments: z.string(),
  }),
})

const messageSchema = z.object({
  role: z.enum(["user", "assistant", "system", "tool"]),
  content: z.string().optional().default(""),
  tool_calls: z.array(toolCallSchema).optional(),
  tool_call_id: z.string().optional(),
})

/**
 * Interval between SSE heartbeat comments. Keeps bytes flowing while the
 * upstream model is still thinking so neither Bun's idle timeout nor any
 * proxy in front (Caddy, Cloudflare) tears the connection down mid-stream —
 * a truncated chunked response surfaces on iOS as URLError -1017.
 */
const HEARTBEAT_INTERVAL_MS = 10_000

/**
 * Merge an SSE byte generator with periodic `: ping` heartbeat comments.
 * A heartbeat is emitted whenever the source has produced nothing for
 * `intervalMs`. SSE comments (lines starting with `:`) are ignored by
 * compliant clients, so they are safe to interleave at any point.
 */
export async function* withSseHeartbeat(
  source: AsyncGenerator<string | Uint8Array>,
  intervalMs: number = HEARTBEAT_INTERVAL_MS,
): AsyncGenerator<string | Uint8Array> {
  const HEARTBEAT = ": ping\n\n"
  let pending = source.next()
  try {
    while (true) {
      let timer: ReturnType<typeof setTimeout> | undefined
      const heartbeat = new Promise<"heartbeat">((resolve) => {
        timer = setTimeout(() => resolve("heartbeat"), intervalMs)
      })
      const winner = await Promise.race([pending, heartbeat])
      clearTimeout(timer)
      if (winner === "heartbeat") {
        yield HEARTBEAT
        continue
      }
      if (winner.done) {
        return
      }
      yield winner.value
      pending = source.next()
    }
  } finally {
    await source.return?.(undefined)
  }
}

const coachBody = z.object({
  model_tier: z.enum(["default", "deep"]).default("default"),
  tool_mode: z.enum(["auto", "required", "none"]).default("auto"),
  messages: z.array(messageSchema).min(1),
  tools: z.array(z.record(z.string(), z.unknown())).optional(),
})

export function coachRoutes(env: Env) {
  const jwt = authJwt({ secret: env.JWT_SECRET })

  const responses = route
    .post("/v1/coach/responses")
    .body(coachBody)
    .use(jwt)
    .handle(async ({ body }) => {
      async function* sseBytes(): AsyncGenerator<string | Uint8Array> {
        const enc = new TextEncoder()
        yield enc.encode(`data: ${JSON.stringify({ type: "response.created" })}\n\n`)
        try {
          const upstreamRequest = {
            modelTier: body.model_tier,
            messages: body.messages,
            toolChoice: body.tool_mode,
            ...(body.tools !== undefined ? { tools: body.tools } : {}),
          }
          const mapChatChunkToResponsesEvents = createChatToResponsesMapper()
          for await (const dataLine of streamUpstreamChat(env, upstreamRequest)) {
            for (const event of mapChatChunkToResponsesEvents(dataLine)) {
              yield formatResponsesSseLine(event)
            }
          }
          yield enc.encode("data: [DONE]\n\n")
        } catch (err) {
          const message = err instanceof Error ? err.message : String(err)
          yield enc.encode(
            `data: ${JSON.stringify({ type: "error", message })}\n\n`,
          )
        }
      }
      return stream(withSseHeartbeat(sseBytes()), {
        headers: {
          "content-type": "text/event-stream; charset=utf-8",
          "cache-control": "no-cache, no-transform",
        },
      })
    })

  return new Hyper({ prefix: "" }).use([responses])
}