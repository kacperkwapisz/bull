import { Hyper, route, stream } from "@hyper/core"
import { authJwt } from "@hyper/auth-jwt"
import { z } from "zod"
import type { Env } from "../lib/env.ts"
import { streamUpstreamChat } from "../services/upstream-chat.ts"
import {
  formatResponsesSseLine,
  mapChatChunkToResponsesEvents,
} from "../services/sse-map-chat-to-responses.ts"

const messageSchema = z.object({
  role: z.enum(["user", "assistant", "system", "tool"]),
  content: z.string(),
})

const coachBody = z.object({
  model_tier: z.enum(["default", "deep"]).default("default"),
  tool_mode: z.enum(["auto", "required", "none"]).default("auto"),
  messages: z.array(messageSchema).min(1),
  tools: z.array(z.record(z.string(), z.unknown())).optional(),
})

export function coachRoutes(env: Env) {
  const jwt = authJwt({
    secret: env.JWT_SECRET,
    allowShortSecret: env.COACH_DEV_AUTH_BYPASS,
  })

  const responses = route
    .post("/v1/coach/responses")
    .body(coachBody)
    .use(jwt)
    .handle(async ({ body }) => {
      async function* sseBytes(): AsyncGenerator<string | Uint8Array> {
        const enc = new TextEncoder()
        yield enc.encode(`data: ${JSON.stringify({ type: "response.created" })}\n\n`)
        try {
          for await (const dataLine of streamUpstreamChat(env, {
            modelTier: body.model_tier,
            messages: body.messages,
            tools: body.tools,
            toolChoice: body.tool_mode,
          })) {
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
      return stream(sseBytes(), {
        headers: {
          "content-type": "text/event-stream; charset=utf-8",
          "cache-control": "no-cache, no-transform",
        },
      })
    })

  return new Hyper({ prefix: "" }).use([responses])
}