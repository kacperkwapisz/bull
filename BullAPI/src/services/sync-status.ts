import { desc, eq } from "drizzle-orm"
import type { Db } from "../db/client.ts"
import { syncRuns, uploadBundles } from "../db/schema.ts"

export async function getSyncStatus(db: Db, userId: string) {
  const rows = await db
    .select({
      id: uploadBundles.id,
      deviceId: uploadBundles.deviceId,
      status: uploadBundles.status,
      createdAt: uploadBundles.createdAt,
      parsedAt: uploadBundles.parsedAt,
      timeframeStart: uploadBundles.timeframeStart,
      timeframeEnd: uploadBundles.timeframeEnd,
      parseError: uploadBundles.parseError,
    })
    .from(uploadBundles)
    .where(eq(uploadBundles.userId, userId))
    .orderBy(desc(uploadBundles.createdAt))
    .limit(1)

  const latest = rows[0] ?? null
  const runs = await db
    .select({
      id: syncRuns.id,
      deviceId: syncRuns.deviceId,
      source: syncRuns.source,
      triggerTimestamp: syncRuns.triggerTimestamp,
      resultTimestamp: syncRuns.resultTimestamp,
      totalPacketUpload: syncRuns.totalPacketUpload,
      uploadRetryCount: syncRuns.uploadRetryCount,
      status: syncRuns.status,
    })
    .from(syncRuns)
    .where(eq(syncRuns.userId, userId))
    .orderBy(desc(syncRuns.resultTimestamp))
    .limit(10)

  return {
    last_successful_upload_at: latest?.createdAt ?? null,
    server_current_through: latest?.timeframeEnd ?? latest?.createdAt ?? null,
    high_watermark: latest?.timeframeEnd ?? latest?.createdAt ?? null,
    latest_upload: latest,
    recent_sync_runs: runs,
  }
}
