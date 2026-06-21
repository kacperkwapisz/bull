import { route, ok, jsonResponse } from "@hyper/core"
import { unlinkSync, existsSync } from "node:fs"
import { sql } from "drizzle-orm"
import { deviceStorePath } from "../services/parse-bundle.ts"
import { getDb } from "../db/client.ts"
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

  return resetStore
}
