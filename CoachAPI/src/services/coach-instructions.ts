/**
 * Composes Bull Coach's system prompt from modular pieces.
 *
 * Each piece is a small file under ./prompt/. Editing prose is local to one
 * file; the tool names in ./prompt/tools.ts must stay in sync with the
 * definitions in ./coach-tools.ts.
 *
 * Edit map:
 *   identity / voice / scope        → ./prompt/{identity,voice,scope}.ts
 *   data provenance + honest gaps   → ./prompt/data-integrity.ts
 *   tool-use discipline             → ./prompt/tools.ts
 *   output format for the app       → ./prompt/format.ts
 *   tone examples                   → ./prompt/examples.ts
 */
import { identity } from "./prompt/identity.ts"
import { voice } from "./prompt/voice.ts"
import { scope } from "./prompt/scope.ts"
import { dataIntegrity } from "./prompt/data-integrity.ts"
import { toolsPolicy } from "./prompt/tools.ts"
import { format } from "./prompt/format.ts"
import { renderExamples } from "./prompt/examples.ts"

export function buildCoachSystemPrompt(): string {
  return [
    "You are Bull Coach, talking to someone about their own body and their own data inside the Bull app.",
    identity,
    voice,
    scope,
    dataIntegrity,
    toolsPolicy,
    format,
    `EXAMPLES (tone calibration, not scripts)\n\n${renderExamples()}`,
  ].join("\n\n")
}

export const COACH_SYSTEM_INSTRUCTIONS = buildCoachSystemPrompt()
