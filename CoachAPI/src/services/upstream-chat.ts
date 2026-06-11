import { COACH_SYSTEM_INSTRUCTIONS } from "./coach-instructions.ts"
import { COACH_TOOL_DEFINITIONS } from "./coach-tools.ts"
import type { Env } from "../lib/env.ts"

export type ModelTier = "default" | "deep"

export interface UpstreamChatRequest {
  modelTier: ModelTier
  messages: { role: "user" | "assistant" | "system" | "tool"; content: string }[]
  tools?: Record<string, unknown>[]
  toolChoice?: "auto" | "required" | "none"
}

function resolveModel(env: Env, tier: ModelTier): string {
  return tier === "deep" ? env.COACH_MODEL_DEEP : env.COACH_MODEL_DEFAULT
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

export async function* streamUpstreamChat(
  env: Env,
  request: UpstreamChatRequest,
): AsyncGenerator<string> {
  const base = env.COACH_UPSTREAM_BASE_URL.replace(/\/$/, "")
  const url = `${base}/chat/completions`
  const model = resolveModel(env, request.modelTier)

  const systemMessages = request.messages.filter((m) => m.role === "system")
  const nonSystem = request.messages.filter((m) => m.role !== "system")
  const instructions =
    systemMessages.map((m) => m.content).join("\n\n").trim() || COACH_SYSTEM_INSTRUCTIONS

  const messages: { role: string; content: string }[] = [
    { role: "system", content: instructions },
    ...nonSystem.map((m) => ({ role: m.role, content: m.content })),
  ]

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
    Authorization: `Bearer ${env.COACH_UPSTREAM_API_KEY}`,
    "Content-Type": "application/json",
    Accept: "text/event-stream",
  }
  if (isGroq(base) || isOrai(base)) {
    headers["User-Agent"] = "bull-coach-api/1"
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