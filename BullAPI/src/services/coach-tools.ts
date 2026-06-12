const emptyParameters = {
  type: "object",
  properties: {},
  required: [] as string[],
  additionalProperties: false,
}

export const COACH_TOOL_DEFINITIONS: Record<string, unknown>[] = [
  {
    type: "function",
    function: {
      name: "load_stats",
      description:
        "Load the current local Bull metric snapshot, readiness status, score summaries, live heart-rate summary, and provenance.",
      parameters: emptyParameters,
    },
  },
  {
    type: "function",
    function: {
      name: "get_activities",
      description:
        "Load the current manual activity, activity detection, movement packet, persistence, and route summaries.",
      parameters: emptyParameters,
    },
  },
  {
    type: "function",
    function: {
      name: "get_capture_sessions",
      description:
        "Load local capture, packet import, Rust core/parser status, last parsed frame, and device evidence coverage.",
      parameters: emptyParameters,
    },
  },
  {
    type: "function",
    function: {
      name: "get_data_gaps",
      description:
        "Load the concrete data gaps and next actions that should block or qualify Coach recommendations.",
      parameters: emptyParameters,
    },
  },
]