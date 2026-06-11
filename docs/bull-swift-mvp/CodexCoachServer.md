# Codex Coach Server Viability

Date: 2026-06-01

## Summary

Shipping a Codex-powered health coach is viable, but not by dropping the current desktop `codex app-server` CLI into the iOS app as a general-purpose sidecar. The viable product shape is:

1. The Swift app owns login, health-data consent, token storage, and all WHOOP/Bull data access.
2. Codex runs in a constrained agent session with no shell, no general filesystem access, and no network tools beyond the model service.
3. Coach data access is exposed through a small read-only tool registry backed by `BullRustBridge` and the existing Rust store/metric methods.
4. Raw session data is summarized and redacted by default; payload hex or decoded frame payloads require explicit debug consent.

For a production App Store iOS app, the lower-risk architecture is either a supported embedded Codex runtime or a remote agent backend. A desktop/macOS sidecar is viable sooner. The current app-server is excellent for proving the protocol and tools, but it is experimental in places and includes coding-agent primitives that are too broad for an in-app health coach unless they are disabled or replaced.

## Current Evidence

- `Coach.md` already sets the right boundary: the MVP Coach must not pretend to be an LLM until there is a real chat backend, privacy policy, and persistence strategy. It also requires local metric citations and no invented metrics.
- Swift already has a JSON-over-C bridge into Rust via `BullRustBridge.request(...)`, and the app model already parses live BLE frames through Rust.
- The Swift tab shell already has a Coach tab, but it is still `CoachPlaceholderView`.
- Rust already exposes most of the needed data plane through bridge methods:
  - `metrics.input_readiness`
  - `capture.list_sessions`
  - `activity.list_sessions`
  - `activity.get_session`
  - `activity.list_metrics`
  - `activity.list_intervals`
  - `activity.metrics_for_session_in_window`
  - `export.raw_timeframe`
- The Rust store already has direct read paths for `activity_sessions_between(...)`, `raw_evidence_between(...)`, and `decoded_frames_between(...)`.
- The generated Codex app-server schema from local `codex-cli 0.135.0` supports ChatGPT login modes, dynamic tool specs, dynamic tool calls, thread/turn lifecycle, sandbox configuration, and approval policy configuration.

## External Constraints

OpenAI's Codex app-server documentation describes app-server as the protocol used by rich clients for authentication, conversation history, approvals, and streamed agent events. It uses bidirectional JSON-RPC 2.0 and supports stdio, WebSocket, Unix socket, and `off` transports. The same docs mark WebSocket transport as experimental and unsupported, and dynamic tool calls are also experimental.

Apple's App Store Review Guideline 2.5.2 is the main mobile packaging concern: apps should be self-contained, must stay inside their container, and may not download, install, or execute code that changes app functionality. Apple's privacy guidance also treats health and fitness data as especially sensitive and requires explicit permission before sharing personal data with third-party AI.

## Decision

Do not plan on shipping the Homebrew-style `codex` executable or an unconstrained `codex app-server` daemon inside the iOS app.

Do plan on one of these paths:

1. **Mac/local prototype:** run `codex app-server` as a localhost or Unix-socket sidecar and route `dynamicTools` to a Swift or local helper registry. This is the fastest way to validate prompts, tool schemas, UX, and redaction.
2. **iOS production, embedded runtime:** use a supported Codex library/runtime build if OpenAI provides one, with shell/filesystem/MCP/plugin capabilities compiled out or unavailable to Coach sessions.
3. **iOS production, remote agent:** host the agent on your backend or through Responses API remote MCP tools. The app authenticates the user, sends scoped health snapshots or tool results, and never exposes raw local storage directly to a model process.

The current repository is best positioned for path 1 as a spike and path 3 as a production fallback.

## Auth Shape

Prefer `ASWebAuthenticationSession` for OpenAI/ChatGPT OAuth-style login rather than a raw embedded `WKWebView`. It gives the user a system-mediated browser login flow and a callback URL back into the app.

Do not rely on manually injecting arbitrary OAuth tokens into Codex's `chatgptAuthTokens` login mode. The local schema marks that mode as unstable and for OpenAI internal use only. Use supported app-server login modes (`chatgpt`, `chatgptDeviceCode`, or API-key mode for non-consumer prototypes) unless OpenAI provides a supported mobile token exchange.

Auth requirements:

- Store tokens only in Keychain.
- Keep auth tokens out of model context and tool outputs.
- Provide explicit logout and revoke/clear local session state.
- Gate Coach startup on a visible health-data consent screen that explains what can be sent to OpenAI or a backend.
- For remote MCP/Responses API architecture, use per-user access tokens or short-lived backend-issued tokens, not app-bundled API keys.

## Coach Tool Registry

All initial tools should be read-only, bounded by time window and row limits, and return provenance. Tool names should be Coach-specific wrappers, not raw bridge method names.

### `load_stats`

Purpose: give the coach a compact, cited health snapshot for a date or window.

Inputs:

```json
{
  "start_time_unix_ms": 0,
  "end_time_unix_ms": 0,
  "include_provenance": true
}
```

Backs onto:

- `metrics.input_readiness`
- packet-derived score and feature summary bridge methods where available
- live BLE heart-rate summary from Swift state
- external sleep/health-sync summaries when present

Output should include metric value, unit, freshness, confidence, source, and why a value is unavailable.

### `get_activities`

Purpose: list normalized activities and optional metrics/intervals.

Inputs:

```json
{
  "start_time_unix_ms": 0,
  "end_time_unix_ms": 0,
  "include_metrics": true,
  "include_intervals": true,
  "limit": 50
}
```

Backs onto:

- `activity.list_sessions`
- `activity.list_metrics`
- `activity.list_intervals`
- `activity.metrics_for_session_in_window`

Output should normalize session id, activity type, start/end/duration, confidence, detection method, sync status, metrics, intervals, and provenance.

### `get_capture_sessions`

Purpose: explain whether device capture exists for a window.

Inputs:

```json
{
  "start_time_unix_ms": 0,
  "end_time_unix_ms": 0,
  "limit": 100
}
```

Backs onto `capture.list_sessions`. This is the right tool for "why is data missing?" before touching raw evidence.

### `get_raw_session_data`

Purpose: support debugging and deep coach explanations when explicitly enabled.

Inputs:

```json
{
  "session_id": "optional",
  "start": "2026-06-01T00:00:00Z",
  "end": "2026-06-02T00:00:00Z",
  "redaction_level": "summary",
  "limit": 200
}
```

Backs onto either:

- direct redacted reads from `raw_evidence_between(...)` and `decoded_frames_between(...)`, or
- a constrained wrapper around `export.raw_timeframe`.

Default `redaction_level` must be `summary`, returning counts, timestamps, device model, sensitivity, warning summaries, hashes, and decoded metric aggregates. Returning `payload_hex` or decoded payload bodies should require:

- explicit debug mode,
- a narrow time window,
- a row limit,
- local audit logging,
- and a visible user consent step.

### `get_data_gaps`

Purpose: let the coach ask, "what data do we need before making a recommendation?"

Backs onto:

- `metrics.input_readiness`
- capture session summaries
- packet-derived feature/score gap summaries
- unavailable health-sync metrics

This tool should drive the Coach empty state and the deterministic "next concrete action" required by `Coach.md`.

## Runtime Boundary

Coach sessions should be created with a health-coach system/developer instruction set that says:

- cite the local tool result for every metric claim;
- say "I do not have that data" when a tool returns missing or stale data;
- do not diagnose, prescribe, or infer medical conditions;
- do not request raw session data unless the user is debugging data quality;
- prefer deterministic app actions: Capture, Sync Health, Calibrate, Import Labels, Open Health page.

App-server settings for a prototype should use:

- read-only sandbox;
- approval policy that never allows shell/file changes for Coach;
- no filesystem tools in the Coach product surface;
- no plugin installation or config mutation from Coach;
- only the Coach dynamic tools listed above.

This matters because app-server's normal product surface includes filesystem APIs, shell-oriented items, MCP server management, plugin management, and approvals. Those are useful for Codex as a coding agent and inappropriate for an in-app health coach.

## Implementation Plan

1. Add a `CoachToolRegistry` in Swift with JSON-schema definitions and async handlers.
2. Add a `BullCoachDataProvider` that wraps `BullRustBridge`, owns the app database path, calls existing bridge methods, and redacts raw outputs.
3. Add a `CoachAuthController` using `ASWebAuthenticationSession` or a supported device-code flow, with Keychain storage and logout.
4. Add a `CodexCoachSessionController` that starts or connects to the chosen runtime, initializes a thread, registers dynamic tools, and routes `item/tool/call` requests into `CoachToolRegistry`.
5. Replace `CoachPlaceholderView` with a gated `CoachView`: signed-out, consent, no-data, loading, chat, and deterministic suggested-question states.
6. Add tests for each tool schema, window validation, row limits, redaction behavior, and missing-data responses.
7. Add an audit log table for Coach tool calls: timestamp, tool name, argument window, row counts, redaction level, and whether raw payload access was approved.

## Spike Plan

The first spike should avoid iOS packaging questions and prove the protocol:

1. Run local `codex app-server --listen unix://` or stdio from a Mac helper.
2. Start a thread with `dynamicTools` for `load_stats`, `get_activities`, and `get_data_gaps`.
3. Serve those tools from a small Swift command-line helper or local Node shim that calls the same JSON-over-C bridge.
4. Validate that the coach can answer five fixed prompts without invented metrics.
5. Capture the exact tool-call JSON, redaction output, and UI states needed for the Swift app.

Exit criteria:

- no uncited metric claims;
- missing data is handled as a first-class answer;
- raw data is not returned unless explicitly requested in debug mode;
- app-server can be driven without exposing shell/file/plugin controls to the user-facing Coach.

## Open Questions

- Will OpenAI support a stable mobile-embeddable Codex runtime, or is app-server intended only as a local desktop process?
- Is ChatGPT login for third-party mobile Codex integrations supported outside the internal `chatgptAuthTokens` path?
- Is this app App Store-bound, TestFlight/internal-only, or macOS-first? The answer changes packaging risk.
- What exact health data may leave the device, and what privacy policy/consent language covers third-party AI processing?
- Should Coach conversation history be local-only, server-backed, or ephemeral by default?

## Recommendation

Proceed with a Mac/local app-server spike to validate the coach tools and prompts, but treat production iOS as requiring either a supported embedded Codex runtime or a remote backend architecture. Build the Coach data boundary now: read-only tools, provenance, redaction, consent, and audit logging. That work is required in all viable architectures and lines up with the existing `Coach.md` acceptance checks.

## References

- OpenAI Codex app-server docs: https://developers.openai.com/codex/app-server
- OpenAI MCP and connectors docs: https://developers.openai.com/api/docs/guides/tools-connectors-mcp
- OpenAI Apps SDK authentication docs: https://developers.openai.com/apps-sdk/build/auth
- Apple App Store Review Guidelines: https://developer.apple.com/app-store/review/guidelines/
- Apple `ASWebAuthenticationSession`: https://developer.apple.com/documentation/authenticationservices/aswebauthenticationsession
