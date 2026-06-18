import { existsSync } from "node:fs"
import type { Env } from "../lib/env.ts"
import { BullCore } from "../lib/bull-core.ts"
import { deviceStorePath } from "./parse-bundle.ts"

/**
 * Read-through query proxy: runs a whitelisted, read-only bull-core method
 * against the connected user's server-side store and returns its result.
 *
 * Display surfaces (nightly sleep history, biometric stream summaries, recorded
 * activity sessions/metrics) read these exactly as the device used to read its
 * own local store, so the device no longer needs to retain decoded history.
 *
 * Safety: the method whitelist is read-only (no writes, no destructive ops), and
 * `database_path` is resolved server-side from the authenticated user id — never
 * accepted from the client. Args are otherwise scalar pass-through (limits, ids,
 * timestamps) validated at the route layer.
 */
const READ_METHODS = new Set<string>([
  "sleep.list_nightly",
  "biometrics.stream_summary",
  "biometrics.spo2_from_raw",
  "activity.list_sessions",
  "activity.list_metrics",
])

// Pure functions that derive a value from their args and need no user store.
const PURE_METHODS = new Set<string>(["biometrics.spo2_from_raw"])

export function isQueryableMethod(method: string): boolean {
  return READ_METHODS.has(method)
}

/**
 * Returns the method result, or `null` when the user has no server store yet
 * (honest empty). Throws `method_not_allowed:<m>` for non-whitelisted methods
 * and `sidecar_unavailable` when the core binary/data dir isn't configured.
 */
export async function runDataQuery(
  env: Env,
  userId: string,
  method: string,
  args: Record<string, unknown>,
): Promise<unknown | null> {
  if (!isQueryableMethod(method)) {
    throw new Error(`method_not_allowed:${method}`)
  }
  if (!env.BULL_CORE_BIN) {
    throw new Error("sidecar_unavailable")
  }
  const core = new BullCore(env.BULL_CORE_BIN)
  try {
    if (PURE_METHODS.has(method)) {
      return await core.request(method, args)
    }
    if (!env.BULL_CORE_DATA_DIR) {
      throw new Error("sidecar_unavailable")
    }
    const dbPath = deviceStorePath(env.BULL_CORE_DATA_DIR, userId)
    if (!existsSync(dbPath)) {
      return null
    }
    // Server-resolved store path always wins over any client-supplied value.
    return await core.request(method, { ...args, database_path: dbPath })
  } finally {
    core.close()
  }
}
