/**
 * Maps OpenAI-compatible chat.completion.chunk SSE payloads to Responses-style
 * events expected by the Bull iOS Coach client.
 */

type ResponsesEvent = { type: string; payload: Record<string, unknown> }

export function mapChatChunkToResponsesEvents(dataLine: string): ResponsesEvent[] {
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

  const toolCalls = delta?.tool_calls as Record<string, unknown>[] | undefined
  if (toolCalls?.length) {
    for (const call of toolCalls) {
      const index = call.index as number | undefined
      const id = (call.id as string | undefined) ?? `tool-${index ?? 0}`
      const fn = call.function as Record<string, unknown> | undefined
      const name = fn?.name as string | undefined
      const args = fn?.arguments as string | undefined
      if (name) {
        events.push({
          type: "response.output_item.added",
          payload: {
            item: {
              type: "function_call",
              id,
              call_id: id,
              name,
              arguments: args ?? "",
            },
            output_index: index ?? 0,
          },
        })
      } else if (args) {
        events.push({
          type: "response.function_call_arguments.delta",
          payload: {
            item_id: id,
            delta: args,
          },
        })
      }
    }
  }

  if (finish === "tool_calls" || finish === "stop") {
    events.push({
      type: "response.completed",
      payload: {
        response: { id: object.id ?? "chatcmpl-mapped" },
      },
    })
  }

  return events
}

export function formatResponsesSseLine(event: ResponsesEvent): string {
  return `data: ${JSON.stringify({ type: event.type, ...event.payload })}\n\n`
}