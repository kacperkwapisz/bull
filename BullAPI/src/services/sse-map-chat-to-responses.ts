/**
 * Maps OpenAI-compatible chat.completion.chunk SSE payloads to Responses-style
 * events expected by the Bull iOS Coach client.
 *
 * Chat Completions streams tool calls incrementally: the `index` is the stable
 * key, while `id` and `function.name` only appear in the first chunk for that
 * index. Subsequent chunks carry only `function.arguments` fragments. There is
 * no per-tool "done" event — completion is signaled by
 * `finish_reason: "tool_calls"` on the final chunk. The mapper therefore keeps
 * per-stream state (keyed by index) so it can emit consistent item ids and
 * synthesize the `*.done` events the Responses client expects.
 */

type ResponsesEvent = { type: string; payload: Record<string, unknown> }

interface AccumulatedToolCall {
  id: string
  name: string
  arguments: string
  outputIndex: number
}

export type ChatToResponsesMapper = (dataLine: string) => ResponsesEvent[]

/**
 * Creates a stateful mapper. One instance must be used per upstream stream.
 */
export function createChatToResponsesMapper(): ChatToResponsesMapper {
  const toolCalls = new Map<number, AccumulatedToolCall>()
  let completedEmitted = false

  return function mapChatChunkToResponsesEvents(dataLine: string): ResponsesEvent[] {
    let object: Record<string, unknown>
    try {
      object = JSON.parse(dataLine) as Record<string, unknown>
    } catch {
      return []
    }

    if (object.error) {
      return [{ type: "error", payload: { error: object.error } }]
    }

    const choice = (object.choices as Record<string, unknown>[] | undefined)?.[0]
    if (!choice) {
      return []
    }

    const delta = choice.delta as Record<string, unknown> | undefined
    const finish = choice.finish_reason as string | undefined
    const events: ResponsesEvent[] = []

    if (delta?.content && typeof delta.content === "string") {
      events.push({
        type: "response.output_text.delta",
        payload: { delta: delta.content },
      })
    }

    const deltaToolCalls = delta?.tool_calls as Record<string, unknown>[] | undefined
    if (deltaToolCalls?.length) {
      for (const call of deltaToolCalls) {
        const index = (call.index as number | undefined) ?? 0
        const fn = call.function as Record<string, unknown> | undefined
        const name = fn?.name as string | undefined
        const args = fn?.arguments as string | undefined

        let entry = toolCalls.get(index)
        if (!entry) {
          // First chunk for this tool call: id and name are present here.
          const id = (call.id as string | undefined) ?? `tool-${index}`
          entry = { id, name: name ?? "function", arguments: "", outputIndex: index }
          toolCalls.set(index, entry)
          events.push({
            type: "response.output_item.added",
            payload: {
              item: {
                type: "function_call",
                id: entry.id,
                call_id: entry.id,
                name: entry.name,
                arguments: "",
              },
              output_index: index,
            },
          })
        } else if (name && entry.name === "function") {
          entry.name = name
        }

        if (typeof args === "string" && args.length > 0) {
          entry.arguments += args
          events.push({
            type: "response.function_call_arguments.delta",
            payload: {
              item_id: entry.id,
              output_index: entry.outputIndex,
              delta: args,
            },
          })
        }
      }
    }

    if (finish === "tool_calls") {
      // Chat Completions has no per-item done event; synthesize them so the
      // Responses client finalizes and executes the accumulated tool calls.
      for (const entry of toolCalls.values()) {
        events.push({
          type: "response.function_call_arguments.done",
          payload: {
            item_id: entry.id,
            output_index: entry.outputIndex,
            arguments: entry.arguments,
          },
        })
        events.push({
          type: "response.output_item.done",
          payload: {
            item: {
              type: "function_call",
              id: entry.id,
              call_id: entry.id,
              name: entry.name,
              arguments: entry.arguments,
            },
            output_index: entry.outputIndex,
          },
        })
      }
    }

    if ((finish === "tool_calls" || finish === "stop") && !completedEmitted) {
      completedEmitted = true
      events.push({
        type: "response.completed",
        payload: {
          response: { id: object.id ?? "chatcmpl-mapped" },
        },
      })
    }

    return events
  }
}

export function formatResponsesSseLine(event: ResponsesEvent): string {
  return `data: ${JSON.stringify({ type: event.type, ...event.payload })}\n\n`
}
