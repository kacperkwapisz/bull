import { and, desc, eq, lt, sql } from "drizzle-orm"
import type { Db } from "../db/client.ts"
import { pushLog, pushTokens, uploadBundles } from "../db/schema.ts"
import type { Env } from "../lib/env.ts"
import { hasApns } from "../lib/env.ts"
import { sendApnsAlert } from "./apns.ts"

export const STALE_SYNC_HOURS = 3
export const STALE_SYNC_COOLDOWN_HOURS = 12

export async function runStaleSyncPush(db: Db, env: Env, now = new Date()): Promise<number> {
  if (!hasApns(env)) return 0

  const staleBefore = new Date(now.getTime() - STALE_SYNC_HOURS * 60 * 60 * 1000)
  const cooldownKey = now.toISOString().slice(0, 10)
  let sent = 0

  const tokenUsers = await db
    .select({ userId: pushTokens.userId })
    .from(pushTokens)
    .groupBy(pushTokens.userId)

  for (const { userId } of tokenUsers) {
    const latest = await db
      .select({ createdAt: uploadBundles.createdAt })
      .from(uploadBundles)
      .where(eq(uploadBundles.userId, userId))
      .orderBy(desc(uploadBundles.createdAt))
      .limit(1)

    const lastUploadAt = latest[0]?.createdAt ?? null
    if (lastUploadAt && lastUploadAt > staleBefore) continue

    const claimed = await db
      .insert(pushLog)
      .values({ userId, kind: "stale_sync", dedupeKey: cooldownKey })
      .onConflictDoNothing()
      .returning({ id: pushLog.id })
    if (claimed.length === 0) continue

    const tokens = await db.select().from(pushTokens).where(eq(pushTokens.userId, userId))
    for (const token of tokens) {
      const result = await sendApnsAlert(env, {
        token: token.token,
        environment: token.environment,
        title: "Bull data behind",
        body: "Open Bull to sync your band data and keep scores fresh.",
        topic: env.APNS_TOPIC,
        payload: { kind: "stale_sync", last_upload_at: lastUploadAt?.toISOString() ?? null },
      })
      const gone =
        result.status === 410 ||
        result.reason === "BadDeviceToken" ||
        result.reason === "Unregistered"
      if (gone) {
        await db
          .delete(pushTokens)
          .where(and(eq(pushTokens.userId, userId), eq(pushTokens.id, token.id)))
      } else if (result.ok) {
        sent += 1
      }
    }
  }

  await db
    .delete(pushLog)
    .where(
      and(
        eq(pushLog.kind, "stale_sync"),
        lt(pushLog.sentAt, sql`now() - interval '${sql.raw(String(STALE_SYNC_COOLDOWN_HOURS))} hours'`),
      ),
    )
  return sent
}
