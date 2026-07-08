import { Hyper, route, ok, jsonResponse } from "@hyper/core"
import { unlinkSync, existsSync } from "node:fs"
import { sql } from "drizzle-orm"
import {
  deviceStorePath,
  requestParseComputeForUser,
  runParseDrainLoop,
} from "../services/parse-bundle.ts"
import { BullCore } from "../lib/bull-core.ts"
import { getDb } from "../db/client.ts"
import { getObjectStore } from "../lib/object-store.ts"
import type { Env } from "../lib/env.ts"

const json = jsonResponse

export function adminRoutes(env: Env) {
  const secret = env.JWT_SECRET

  const resetStore = route
    .delete("/admin/reset-store/:userId")
    .handle(async ({ params, req }) => {
      // Simple shared-secret auth via Bearer token.
      const auth = req.headers.get("authorization")
      if (auth !== `Bearer ${secret}`) {
        return json(401, { error: "unauthorized" })
      }
      const userId = (params as { userId?: string }).userId
      if (!userId || userId.length < 10) {
        return json(400, { error: "invalid userId" })
      }

      const dataDir = env.BULL_CORE_DATA_DIR
      if (!dataDir) return json(500, { error: "BULL_CORE_DATA_DIR not configured" })

      const storePath = deviceStorePath(dataDir, userId)
      const existed = existsSync(storePath)
      if (existed) {
        unlinkSync(storePath)
      }

      // Re-queue all bundles for this user so they re-import with the new parser.
      const db = getDb(env)
      let requeued = 0
      if (db) {
        const result = await db.execute(
          sql`UPDATE upload_bundles SET status = 'pending', parse_error = NULL WHERE user_id = ${userId} AND status IN ('parsed', 'failed')`,
        )
        requeued = (result as any).rowCount ?? (result as any).length ?? 0
      }

      return ok({ deleted: existed, path: storePath, requeued })
    })

  const drain = route.post("/admin/drain").handle(async ({ req }) => {
    const auth = req.headers.get("authorization")
    if (auth !== `Bearer ${secret}`) {
      return json(401, { error: "unauthorized" })
    }
    if (!env.BULL_CORE_BIN || !env.BULL_CORE_DATA_DIR) {
      return json(503, { error: "parse_not_configured" })
    }
    const db = getDb(env)
    const store = getObjectStore(env)
    if (!db || !store) return json(503, { error: "persistence_unavailable" })

    const url = new URL(req.url)
    const limitParam = url.searchParams.get("limit")
    const limit = limitParam ? Math.min(1000, Math.max(50, Number(limitParam) || 500)) : undefined
    const userId = url.searchParams.get("userId")?.trim()
    const forceCompute = url.searchParams.get("forceCompute") === "1"

    if (userId) requestParseComputeForUser(userId)

    const drainOpts: { forceCompute: boolean; limit?: number; maxBatches?: number; forceComputeUserId?: string } = {
      forceCompute: forceCompute || Boolean(userId),
      maxBatches: 100,
    }
    if (userId) drainOpts.forceComputeUserId = userId
    if (limit !== undefined) drainOpts.limit = limit
    const result = await runParseDrainLoop(
      db,
      store,
      { binaryPath: env.BULL_CORE_BIN, dataDir: env.BULL_CORE_DATA_DIR, env },
      drainOpts,
    )
    return ok(result)
  })

  const debug = route.get("/admin/debug").handle(async ({ req }) => {
    const auth = req.headers.get("authorization")
    if (auth !== `Bearer ${secret}`) return json(401, { error: "unauthorized" })
    if (!env.BULL_CORE_BIN || !env.BULL_CORE_DATA_DIR) return json(503, { error: "not_configured" })
    const userId = new URL(req.url).searchParams.get("userId")?.trim()
    if (!userId) return json(400, { error: "userId required" })
    const dbPath = deviceStorePath(env.BULL_CORE_DATA_DIR, userId)
    const core = new BullCore(env.BULL_CORE_BIN)
    try {
      const overview = await core.request("debug.db_overview", { database_path: dbPath })
      return ok(overview)
    } finally {
      core.close()
    }
  })

  const debugSleep = route.get("/admin/debug/sleep").handle(async ({ req }) => {
    const auth = req.headers.get("authorization")
    if (auth !== `Bearer ${secret}`) return json(401, { error: "unauthorized" })
    if (!env.BULL_CORE_BIN || !env.BULL_CORE_DATA_DIR) return json(503, { error: "not_configured" })
    const url = new URL(req.url)
    const userId = url.searchParams.get("userId")?.trim()
    const start = url.searchParams.get("start")?.trim()
    const end = url.searchParams.get("end")?.trim()
    if (!userId) return json(400, { error: "userId required" })
    if (!start || !end) return json(400, { error: "start and end required" })
    const dbPath = deviceStorePath(env.BULL_CORE_DATA_DIR, userId)
    const core = new BullCore(env.BULL_CORE_BIN)
    try {
      const report = await core.request("metrics.sleep_score_from_features", {
        database_path: dbPath,
        start,
        end,
        algorithm_id: "bull.sleep.v0",
        algorithm_version: "0.1.0",
        min_owned_captures: 1,
        require_trusted_evidence: false,
        sleep_need_minutes: 480.0,
        low_motion_threshold_0_to_1: 0.05,
        disturbance_motion_threshold_0_to_1: 0.20,
        target_midpoint_minutes_since_midnight: 180.0,
        persist_algorithm_run: false,
        persist_nightly: false,
      })
      return ok(report)
    } finally {
      core.close()
    }
  })

  return new Hyper({ prefix: "" }).use([resetStore, drain, debug, debugSleep])
}
