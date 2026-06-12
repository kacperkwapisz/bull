import { describe, expect, test } from "bun:test"
import { createChatToResponsesMapper } from "../src/services/sse-map-chat-to-responses.ts"

type Event = { type: string; payload: Record<string, unknown> }

/** Feed OpenAI-style chat.completion.chunk lines through one mapper instance. */
function run(chunks: Record<string, unknown>[]): Event[] {
  const map = createChatToResponsesMapper()
  return chunks.flatMap((c) => map(JSON.stringify(c)) as unknown as Event[])
}

function chunk(delta: Record<string, unknown>, finish: string | null = null) {
  return { id: "chatcmpl-1", choices: [{ index: 0, delta, finish_reason: finish }] }
}

describe("chat->responses tool call mapping", () => {
  test("streams id/name only on first chunk, args fragments after, and synthesizes done", () => {
    const events = run([
      // first chunk: id + name, empty args (OpenAI streaming format)
      chunk({ tool_calls: [{ index: 0, id: "call_abc", type: "function", function: { name: "load_stats", arguments: "" } }] }),
      // arg fragments — no id, only index
      chunk({ tool_calls: [{ index: 0, function: { arguments: '{"a"' } }] }),
      chunk({ tool_calls: [{ index: 0, function: { arguments: ":1}" } }] }),
      // final chunk signals completion via finish_reason
      chunk({}, "tool_calls"),
    ])

    const added = events.filter((e) => e.type === "response.output_item.added")
    expect(added).toHaveLength(1)
    expect((added[0]!.payload.item as Record<string, unknown>).id).toBe("call_abc")
    expect((added[0]!.payload.item as Record<string, unknown>).name).toBe("load_stats")

    // every arguments delta must carry the SAME consistent item id
    const argDeltas = events.filter((e) => e.type === "response.function_call_arguments.delta")
    expect(argDeltas).toHaveLength(2)
    expect(argDeltas.every((e) => e.payload.item_id === "call_abc")).toBe(true)

    // synthesized done events carry the fully accumulated arguments
    const done = events.find((e) => e.type === "response.output_item.done")
    expect(done).toBeDefined()
    const item = done!.payload.item as Record<string, unknown>
    expect(item.id).toBe("call_abc")
    expect(item.name).toBe("load_stats")
    expect(item.arguments).toBe('{"a":1}')

    const argsDone = events.find((e) => e.type === "response.function_call_arguments.done")
    expect(argsDone?.payload.arguments).toBe('{"a":1}')

    // exactly one completion event
    expect(events.filter((e) => e.type === "response.completed")).toHaveLength(1)
  })

  test("plain text completion emits a single response.completed and no tool done", () => {
    const events = run([
      chunk({ content: "Hello" }),
      chunk({ content: " world" }),
      chunk({}, "stop"),
    ])
    expect(events.filter((e) => e.type === "response.output_text.delta")).toHaveLength(2)
    expect(events.filter((e) => e.type === "response.output_item.done")).toHaveLength(0)
    expect(events.filter((e) => e.type === "response.completed")).toHaveLength(1)
  })
})
