import { COACH_SYSTEM_INSTRUCTIONS } from "./coach-instructions.ts"
import { COACH_TOOL_DEFINITIONS } from "./coach-tools.ts"
import type { Env } from "../lib/env.ts"

export type ModelTier = "default" | "deep"

export interface UpstreamToolCall {
  id: string
  type?: "function" | undefined
  function: { name: string; arguments: string }
}

export interface UpstreamChatMessage {
  role: "user" | "assistant" | "system" | "tool"
  content: string
  tool_calls?: UpstreamToolCall[] | undefined
  tool_call_id?: string | undefined
}

export interface UpstreamChatRequest {
  modelTier: ModelTier
  messages: UpstreamChatMessage[]
  tools?: Record<string, unknown>[]
  toolChoice?: "auto" | "required" | "none"
}

function resolveModel(env: Env, tier: ModelTier): string {
  return tier === "deep" ? env.BULL_MODEL_DEEP : env.BULL_MODEL_DEFAULT
}

function isZen(baseURL: string): boolean {
  return baseURL.includes("opencode.ai")
}

function isGroq(baseURL: string): boolean {
  return baseURL.includes("groq.com")
}

function isOrai(baseURL: string): boolean {
  return baseURL.includes("oraiapi.com")
}

function normalizeToolsForOpenAICompat(tools: Record<string, unknown>[] | undefined): unknown[] | undefined {
  if (!tools?.length) {
    return undefined
  }
  return tools.map((tool) => {
    if (tool.type === "function" && "function" in tool && typeof tool.function === "object") {
      return tool
    }
    const legacy = tool as { name?: string; description?: string; parameters?: unknown }
    if (legacy.name) {
      return {
        type: "function",
        function: {
          name: legacy.name,
          description: legacy.description ?? "",
          parameters: legacy.parameters ?? { type: "object", properties: {} },
        },
      }
    }
    return tool
  })
}

/**
 * Assembles the OpenAI-compatible message array: a single leading system
 * message (callers' system turns are merged, falling back to the Coach
 * instructions) followed by the conversation, preserving the multi-turn tool
 * protocol fields (`tool_calls` on assistant turns, `tool_call_id` on results).
 */
export function buildUpstreamMessages(
  messages: UpstreamChatMessage[],
): Record<string, unknown>[] {
  const systemMessages = messages.filter((m) => m.role === "system")
  const nonSystem = messages.filter((m) => m.role !== "system")
  const instructions =
    systemMessages.map((m) => m.content).join("\n\n").trim() || COACH_SYSTEM_INSTRUCTIONS

  return [
    { role: "system", content: instructions },
    ...nonSystem.map((m) => {
      const out: Record<string, unknown> = { role: m.role, content: m.content ?? "" }
      if (m.tool_calls?.length) {
        out.tool_calls = m.tool_calls.map((call) => ({
          id: call.id,
          type: call.type ?? "function",
          function: { name: call.function.name, arguments: call.function.arguments },
        }))
      }
      if (m.tool_call_id) {
        out.tool_call_id = m.tool_call_id
      }
      return out
    }),
  ]
}

export async function* streamUpstreamChat(
  env: Env,
  request: UpstreamChatRequest,
): AsyncGenerator<string> {
  const base = env.BULL_UPSTREAM_BASE_URL.replace(/\/$/, "")
  const url = `${base}/chat/completions`
  const model = resolveModel(env, request.modelTier)

  const messages = buildUpstreamMessages(request.messages)

  const body: Record<string, unknown> = {
    model,
    messages,
    stream: true,
  }

  const tools = normalizeToolsForOpenAICompat(request.tools ?? COACH_TOOL_DEFINITIONS)
  if (tools?.length && request.toolChoice !== "none") {
    body.tools = tools
    body.tool_choice = request.toolChoice ?? "auto"
    body.parallel_tool_calls = false
  }

  const headers: Record<string, string> = {
    Authorization: `Bearer ${env.BULL_UPSTREAM_API_KEY}`,
    "Content-Type": "application/json",
    Accept: "text/event-stream",
  }
  if (isGroq(base) || isOrai(base)) {
    headers["User-Agent"] = "bull-api/1"
  }
  if (isZen(base)) {
    headers.originator = "bull-swift"
  }

  const upstream = await fetch(url, {
    method: "POST",
    headers,
    body: JSON.stringify(body),
  })

  if (!upstream.ok) {
    const text = await upstream.text()
    throw new Error(`Upstream ${upstream.status}: ${text.slice(0, 2000)}`)
  }
  if (!upstream.body) {
    throw new Error("Upstream returned no body")
  }

  const reader = upstream.body.getReader()
  const decoder = new TextDecoder()
  let buffer = ""

  while (true) {
    const { done, value } = await reader.read()
    if (done) {
      break
    }
    buffer += decoder.decode(value, { stream: true })
    const parts = buffer.split("\n\n")
    buffer = parts.pop() ?? ""
    for (const part of parts) {
      const lines = part.split("\n")
      for (const line of lines) {
        const trimmed = line.trim()
        if (!trimmed.startsWith("data:")) {
          continue
        }
        const data = trimmed.slice(5).trim()
        if (data === "[DONE]") {
          return
        }
        yield data
      }
    }
  }
}