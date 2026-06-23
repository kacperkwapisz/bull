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
  "activity.list_sessions",
  "activity.list_metrics",
])

export function isQueryableMethod(method: string): boolean {
  return READ_METHODS.has(method)
}

/**
 * Clear cached sleep scores for a user so they recompute from raw sensor data
 * on the next read. Returns the bridge result or null if no store exists.
 */
export async function clearCachedSleepScores(
  env: Env,
  userId: string,
): Promise<unknown | null> {
  if (!env.BULL_CORE_BIN || !env.BULL_CORE_DATA_DIR) {
    throw new Error("sidecar_unavailable")
  }
  const core = new BullCore(env.BULL_CORE_BIN)
  try {
    const dbPath = deviceStorePath(env.BULL_CORE_DATA_DIR, userId)
    if (!existsSync(dbPath)) return null
    return await core.request("sleep.clear_cached_scores", { database_path: dbPath })
  } finally {
    core.close()
  }
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
  if (!env.BULL_CORE_DATA_DIR) {
    throw new Error("sidecar_unavailable")
  }
  const core = new BullCore(env.BULL_CORE_BIN)
  try {
    const dbPath = deviceStorePath(env.BULL_CORE_DATA_DIR, userId)
    if (!existsSync(dbPath)) {
      return null
    }
    // Server-resolved store path always wins over any client-supplied value.
    const result = await core.request(method, { ...args, database_path: dbPath })

    // Single round-trip enrichment: the stream summary carries the latest raw
    // SpO2 red/ir pair; convert it to a percentage on the same core process so
    // the client needs one request and the SpO2 formula stays single-sourced in
    // bull-core (no on-device or duplicated conversion).
    if (method === "biometrics.stream_summary" && result && typeof result === "object") {
      const r = result as Record<string, unknown>
      const red = typeof r["latest_spo2_red"] === "number" ? r["latest_spo2_red"] : undefined
      const ir = typeof r["latest_spo2_ir"] === "number" ? r["latest_spo2_ir"] : undefined
      if (red !== undefined && ir !== undefined) {
        const conv = (await core.request("biometrics.spo2_from_raw", { red, ir })) as
          | Record<string, unknown>
          | null
        r["latest_spo2_pct"] =
          conv && typeof conv["spo2_pct"] === "number" ? conv["spo2_pct"] : null
      }
    }
    return result
  } finally {
    core.close()
  }
}
