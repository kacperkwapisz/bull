import { describe, expect, test } from "bun:test"
import { buildUpstreamMessages } from "../src/services/upstream-chat.ts"

describe("buildUpstreamMessages", () => {
  test("preserves the multi-turn tool protocol fields", () => {
    const out = buildUpstreamMessages([
      { role: "user", content: "How am I doing?" },
      {
        role: "assistant",
        content: "",
        tool_calls: [
          { id: "call_abc", function: { name: "load_stats", arguments: "{}" } },
        ],
      },
      { role: "tool", content: '{"hr":62}', tool_call_id: "call_abc" },
    ])

    // leading system message is always injected
    expect(out[0]!.role).toBe("system")

    const assistant = out.find((m) => m.role === "assistant") as Record<string, unknown>
    const toolCalls = assistant.tool_calls as Record<string, unknown>[]
    expect(toolCalls[0]!.id).toBe("call_abc")
    expect(toolCalls[0]!.type).toBe("function")
    expect((toolCalls[0]!.function as Record<string, unknown>).name).toBe("load_stats")

    const toolMsg = out.find((m) => m.role === "tool") as Record<string, unknown>
    expect(toolMsg.tool_call_id).toBe("call_abc")
    expect(toolMsg.content).toBe('{"hr":62}')
  })

  test("merges caller system turns ahead of the conversation", () => {
    const out = buildUpstreamMessages([
      { role: "system", content: "Be terse." },
      { role: "user", content: "hi" },
    ])
    expect(out[0]!.role).toBe("system")
    expect(out[0]!.content).toBe("Be terse.")
    expect(out[1]!.role).toBe("user")
  })
})
