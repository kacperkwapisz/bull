/**
 * Ingest of device export bundles.
 *
 * The raw bundle is the source of record: it is stored verbatim in object
 * storage (S3/R2) and tracked in upload_bundles (deduped per user by sha256).
 * A device-supplied summary — derived from the same connected-device sensor
 * store the Bull app reads for its own UI — is projected into curated tables
 * (recovery, sleep, SpO2) so the web app can query them and so uploads are
 * inspectable. Every value originates from the device's own live sensors;
 * nothing here is imported from third-party health stores. The summary is
 * re-derivable from the raw bundle, so the projection can always be rebuilt.
 *
 * Ordering: bytes are written to the bucket first (idempotent by key), then the
 * DB row is recorded. A failure after the put leaves a harmless orphan object
 * that the next identical upload reuses — never a DB row without bytes.
 */

import { z } from "zod"
import { sql } from "drizzle-orm"
import type { Db } from "../db/client.ts"
import { dailyRecovery, dailySleep, spo2Samples, syncRuns, uploadBundles } from "../db/schema.ts"
import { bundleObjectKey, type ObjectStore } from "../lib/object-store.ts"

const dayString = z.string().regex(/^\d{4}-\d{2}-\d{2}$/)

export const bundleSummarySchema = z.object({
  timeframe: z
    .object({ start: z.string().datetime().optional(), end: z.string().datetime().optional() })
    .optional(),
  recovery: z
    .array(
      z.object({
        day: dayString,
        recovery_score: z.number().nullable().optional(),
        hrv_ms: z.number().nullable().optional(),
        resting_hr_bpm: z.number().nullable().optional(),
        raw: z.record(z.string(), z.unknown()).optional(),
      }),
    )
    .optional(),
  sleep: z
    .array(
      z.object({
        day: dayString,
        sleep_score: z.number().nullable().optional(),
        total_sleep_minutes: z.number().nullable().optional(),
        rem_minutes: z.number().nullable().optional(),
        deep_minutes: z.number().nullable().optional(),
        light_minutes: z.number().nullable().optional(),
        awake_minutes: z.number().nullable().optional(),
        raw: z.record(z.string(), z.unknown()).optional(),
      }),
    )
    .optional(),
  spo2: z
    .array(
      z.object({
        recorded_at: z.string().datetime(),
        spo2: z.number().nullable().optional(),
        raw: z.record(z.string(), z.unknown()).optional(),
      }),
    )
    .optional(),
})

export type BundleSummary = z.infer<typeof bundleSummarySchema>

function sha256Hex(bytes: Uint8Array): string {
  const hasher = new Bun.CryptoHasher("sha256")
  hasher.update(bytes)
  return hasher.digest("hex")
}

export interface IngestResult {
  readonly bundleId: string
  readonly checksum: string
  readonly status: "parsed" | "pending"
  readonly deduped: boolean
}

export interface IngestInput {
  readonly userId: string
  readonly deviceId?: string
  readonly bytes: Uint8Array
  readonly contentType?: string
  readonly summary?: BundleSummary
  readonly source?: string
  readonly triggeredAt?: Date
  readonly packetCount?: number
  readonly retryCount?: number
}

export async function ingestBundle(
  db: Db,
  store: ObjectStore,
  input: IngestInput,
): Promise<IngestResult> {
  const checksum = sha256Hex(input.bytes)

  const existing = await db
    .select({ id: uploadBundles.id, status: uploadBundles.status })
    .from(uploadBundles)
    .where(sql`${uploadBundles.userId} = ${input.userId} and ${uploadBundles.checksum} = ${checksum}`)
    .limit(1)
  if (existing.length > 0 && existing[0]) {
    return {
      bundleId: existing[0].id,
      checksum,
      status: existing[0].status === "parsed" ? "parsed" : "pending",
      deduped: true,
    }
  }

  const contentType = input.contentType ?? "application/octet-stream"
  const storageKey = bundleObjectKey(input.userId, checksum)
  await store.put(storageKey, input.bytes, contentType)

  const tf = input.summary?.timeframe
  const inserted = await db
    .insert(uploadBundles)
    .values({
      userId: input.userId,
      ...(input.deviceId !== undefined ? { deviceId: input.deviceId } : {}),
      checksum,
      byteSize: input.bytes.byteLength,
      status: "pending",
      storageKey,
      contentType,
      ...(tf?.start ? { timeframeStart: new Date(tf.start) } : {}),
      ...(tf?.end ? { timeframeEnd: new Date(tf.end) } : {}),
    })
    .returning({ id: uploadBundles.id })
  const row = inserted[0]
  if (!row) throw new Error("failed to record upload bundle")
  const bundleId = row.id

  await recordSyncRun(db, input, bundleId)

  if (!input.summary) {
    return { bundleId, checksum, status: "pending", deduped: false }
  }

  await projectSummary(db, input.userId, bundleId, input.summary)
  await db
    .update(uploadBundles)
    .set({ status: "parsed", parsedAt: sql`now()` })
    .where(sql`${uploadBundles.id} = ${bundleId}`)

  return { bundleId, checksum, status: "parsed", deduped: false }
}

async function recordSyncRun(db: Db, input: IngestInput, bundleId: string): Promise<void> {
  await db.insert(syncRuns).values({
    userId: input.userId,
    ...(input.deviceId !== undefined ? { deviceId: input.deviceId } : {}),
    uploadBundleId: bundleId,
    source: input.source ?? "upload_bundle",
    triggerTimestamp: input.triggeredAt ?? new Date(),
    totalPacketUpload: input.packetCount ?? 0,
    uploadRetryCount: input.retryCount ?? 0,
    status: "uploaded",
  })
}

async function projectSummary(
  db: Db,
  userId: string,
  bundleId: string,
  summary: BundleSummary,
): Promise<void> {
  for (const r of summary.recovery ?? []) {
    await db
      .insert(dailyRecovery)
      .values({
        userId,
        sourceBundleId: bundleId,
        day: r.day,
        recoveryScore: r.recovery_score ?? null,
        hrvMs: r.hrv_ms ?? null,
        restingHrBpm: r.resting_hr_bpm ?? null,
        raw: r.raw ?? null,
      })
      .onConflictDoUpdate({
        target: [dailyRecovery.userId, dailyRecovery.day],
        set: {
          sourceBundleId: bundleId,
          recoveryScore: r.recovery_score ?? null,
          hrvMs: r.hrv_ms ?? null,
          restingHrBpm: r.resting_hr_bpm ?? null,
          raw: r.raw ?? null,
        },
      })
  }

  for (const s of summary.sleep ?? []) {
    await db
      .insert(dailySleep)
      .values({
        userId,
        sourceBundleId: bundleId,
        day: s.day,
        sleepScore: s.sleep_score ?? null,
        totalSleepMinutes: s.total_sleep_minutes ?? null,
        remMinutes: s.rem_minutes ?? null,
        deepMinutes: s.deep_minutes ?? null,
        lightMinutes: s.light_minutes ?? null,
        awakeMinutes: s.awake_minutes ?? null,
        raw: s.raw ?? null,
      })
      .onConflictDoUpdate({
        target: [dailySleep.userId, dailySleep.day],
        set: {
          sourceBundleId: bundleId,
          sleepScore: s.sleep_score ?? null,
          totalSleepMinutes: s.total_sleep_minutes ?? null,
          remMinutes: s.rem_minutes ?? null,
          deepMinutes: s.deep_minutes ?? null,
          lightMinutes: s.light_minutes ?? null,
          awakeMinutes: s.awake_minutes ?? null,
          raw: s.raw ?? null,
        },
      })
  }

  for (const p of summary.spo2 ?? []) {
    await db
      .insert(spo2Samples)
      .values({
        userId,
        sourceBundleId: bundleId,
        recordedAt: new Date(p.recorded_at),
        spo2: p.spo2 ?? null,
        raw: p.raw ?? null,
      })
      .onConflictDoNothing({ target: [spo2Samples.userId, spo2Samples.recordedAt] })
  }
}
