/**
 * Server-side parse pipeline (thin-client keystone).
 *
 * Replays, server-side, the exact pipeline the device runs: it reads a pending
 * upload bundle from object storage, decompresses it, imports the raw frames
 * into a per-user `bull-core` SQLite store via the sidecar, runs the single
 * `metrics.run_pipeline` compute entry point, then flips the bundle to
 * `parsed`. The compute uses the same parser/algorithms as the device, so there
 * is one source of truth and no device/server drift.
 *
 * Bundles are zlib-DEFLATE — Apple's `NSData.compressed(using: .zlib)` emits
 * raw DEFLATE (RFC 1951, no header), so we inflate with `inflateRawSync`.
 */

import { inflateRawSync } from "node:zlib"
import { and, eq } from "drizzle-orm"
import type { Db } from "../db/client.ts"
import { dailySleep, uploadBundles } from "../db/schema.ts"
import { getBundleForUser } from "./data-read.ts"
import { ingestMetrics, metricsPushSchema } from "./metrics-ingest.ts"
import { BullCore } from "../lib/bull-core.ts"
import type { ObjectStore } from "../lib/object-store.ts"

const DEVICE_MODEL = "WHOOP 5.0 Bull"
const LOCAL_BIOMETRIC_DEVICE_ID = "bull-local"

// Nightly sleep-score tuning. Mirrors the device's read-time score call
// (HealthDataStore+Utilities.swift `sleepScoreReport`); kept here so the server
// produces the same per-night scores. "0000"/"9999" are full-range scan bounds
// (not timestamps), so no window derivation is needed.
const SLEEP_SCORE_ARGS = {
  start: "0000",
  end: "9999",
  min_owned_captures: 2,
  require_trusted_evidence: false,
  sleep_need_minutes: 480.0,
  low_motion_threshold_0_to_1: 0.05,
  disturbance_motion_threshold_0_to_1: 0.2,
  target_midpoint_minutes_since_midnight: 180.0,
  history_import_in_progress: false,
  algorithm_id: "bull.sleep.v1",
  persist_nightly: true,
} as const

// Read-time score tuning, identical to the device's recovery/strain/stress
// calls (HealthDataStore+Utilities.swift). "0000"/"9999" are full-range scan
// bounds, so the score reflects the most recent computable day.
const SCORE_ARGS = {
  start: "0000",
  end: "9999",
  min_owned_captures: 2,
  require_trusted_evidence: false,
  hrv_start: "0000",
  hrv_end: "9999",
  hrv_baseline_start: "0000",
  hrv_baseline_end: "9999",
  resting_start: "0000",
  resting_end: "9999",
  sleep_start: "0000",
  sleep_end: "9999",
  prior_strain_start: "0000",
  prior_strain_end: "9999",
  resting_baseline_min_days: 3,
  hrv_min_rr_intervals_to_compute: 2,
  hrv_baseline_min_days: 3,
  sleep_need_minutes: 480.0,
  low_motion_threshold_0_to_1: 0.05,
  disturbance_motion_threshold_0_to_1: 0.2,
  target_midpoint_minutes_since_midnight: 180.0,
  prior_strain_resting_baseline_min_days: 3,
} as const

/** Pull the 0-100 score out of a *_score_from_features report. */
function scoreValue(report: unknown): number | null {
  const output = (report as { score_result?: { output?: { score_0_to_100?: unknown } } })
    ?.score_result?.output?.score_0_to_100
  return typeof output === "number" ? output : null
}

/** Most recent day present in the curated export, or null when empty. */
function latestExportDay(body: Record<string, unknown> | undefined): string | null {
  if (!body) return null
  const days: string[] = []
  for (const key of ["sleep", "vitals"]) {
    const rows = body[key]
    if (Array.isArray(rows)) {
      for (const row of rows) {
        const day = (row as { day?: unknown }).day
        if (typeof day === "string") days.push(day)
      }
    }
  }
  return days.length > 0 ? days.sort().at(-1)! : null
}

interface BundleFrameLine {
  evidence_id: string
  captured_at: string
  payload_hex: string
  sha256?: string
}

export interface ParseBundleConfig {
  binaryPath: string
  dataDir: string
}

export interface ParseBundleResult {
  bundleId: string
  frameCount: number
  status: "parsed" | "failed"
  reports?: Record<string, unknown>
  ingested?: {
    readonly recovery: number
    readonly sleep: number
    readonly strain: number
    readonly stress: number
    readonly energy: number
    readonly vitals: number
    readonly spo2: number
  }
  error?: string
}

/** UTC day/hour windows for the run_pipeline call, derived from server "now". */
function pipelineWindows(now: Date) {
  const dayStart = new Date(Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate()))
  const dayEnd = new Date(dayStart.getTime() + 86_400_000)
  const hourStart = new Date(
    Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate(), now.getUTCHours()),
  )
  const hourEnd = new Date(hourStart.getTime() + 3_600_000)
  const dateKey = (d: Date) => d.toISOString().slice(0, 10)
  const window = (start: Date, end: Date) => ({
    date_key: dateKey(start),
    timezone: "UTC",
    start_iso: start.toISOString(),
    end_iso: end.toISOString(),
    start_time_unix_ms: start.getTime(),
    end_time_unix_ms: end.getTime(),
  })
  return { daily: window(dayStart, dayEnd), hourly: window(hourStart, hourEnd) }
}

function deviceStorePath(dataDir: string, userId: string): string {
  return `${dataDir.replace(/\/$/, "")}/${userId}.sqlite`
}

/**
 * Parse all still-pending bundles for a user (bounded), oldest first. Called
 * fire-and-forget after an upload so freshly-arrived data is computed promptly,
 * and so a bundle missed by a crash/restart is caught up on the next upload.
 * Errors are isolated per bundle; a failure marks that bundle `failed` and the
 * sweep continues.
 */
export async function parsePendingBundles(
  db: Db,
  store: ObjectStore,
  config: ParseBundleConfig,
  userId: string,
  limit = 25,
): Promise<ParseBundleResult[]> {
  const pending = await db
    .select({ id: uploadBundles.id })
    .from(uploadBundles)
    .where(and(eq(uploadBundles.userId, userId), eq(uploadBundles.status, "pending")))
    .orderBy(uploadBundles.createdAt)
    .limit(limit)
  const results: ParseBundleResult[] = []
  for (const { id } of pending) {
    results.push(await parseBundle(db, store, config, userId, id))
  }
  return results
}

/**
 * Drain pending bundles across ALL users (bounded, oldest first). Driven by a
 * server interval so a backlog clears on its own without waiting for an upload
 * to trigger a per-user sweep. Returns the number successfully parsed.
 */
export async function parseAllPending(
  db: Db,
  store: ObjectStore,
  config: ParseBundleConfig,
  limit = 25,
): Promise<number> {
  const pending = await db
    .select({ id: uploadBundles.id, userId: uploadBundles.userId })
    .from(uploadBundles)
    .where(eq(uploadBundles.status, "pending"))
    .orderBy(uploadBundles.createdAt)
    .limit(limit)
  let parsed = 0
  for (const { id, userId } of pending) {
    const result = await parseBundle(db, store, config, userId, id)
    if (result.status === "parsed") parsed += 1
  }
  return parsed
}

/**
 * Parse one bundle end-to-end. Idempotent: re-parsing a bundle re-imports its
 * frames (deduped in bull-core) and re-runs compute (idempotent by timestamp).
 */
export async function parseBundle(
  db: Db,
  store: ObjectStore,
  config: ParseBundleConfig,
  userId: string,
  bundleId: string,
): Promise<ParseBundleResult> {
  const bundle = await getBundleForUser(db, userId, bundleId)
  if (!bundle) {
    return { bundleId, frameCount: 0, status: "failed", error: "bundle_not_found" }
  }

  const core = new BullCore(config.binaryPath)
  try {
    const compressed = await store.get(bundle.storageKey)
    const jsonl = inflateRawSync(compressed).toString("utf8")
    const frames: BundleFrameLine[] = jsonl
      .split("\n")
      .filter((line) => line.trim().length > 0)
      .map((line) => JSON.parse(line) as BundleFrameLine)

    const dbPath = deviceStorePath(config.dataDir, userId)
    const importFrames = frames.map((frame) => ({
      evidence_id: frame.evidence_id,
      source: "server.parse",
      captured_at: frame.captured_at,
      device_model: DEVICE_MODEL,
      frame_hex: frame.payload_hex,
      sensitivity: "user-owned-capture",
      device_type: "BULL" as const,
    }))

    await core.request("capture.import_frame_batch", {
      database_path: dbPath,
      frames: importFrames,
      include_timeline_rows: false,
      include_results: false,
    })

    const windows = pipelineWindows(new Date())
    const pipeline = await core.request<{ reports?: Record<string, unknown> }>(
      "metrics.run_pipeline",
      {
        database_path: dbPath,
        device_id: LOCAL_BIOMETRIC_DEVICE_ID,
        daily_window: windows.daily,
        hourly_window: windows.hourly,
      },
    )

    // Persist per-night sleep scores into the store's daily tables so the
    // curated export below carries them (run_pipeline does ingest+rollups; the
    // sleep score is a separate read-time step that also writes the nightly row).
    // Keep the full report — the app reads it verbatim instead of recomputing.
    const sleepReport = await core.request<Record<string, unknown>>(
      "metrics.sleep_score_from_features",
      { database_path: dbPath, ...SLEEP_SCORE_ARGS },
    )

    // Read the curated per-day rows bull-core just computed and fold them into
    // the Postgres result tables via the same idempotent upsert the device's
    // curated sync uses. Populates dailySleep (score + stages) and vitalsDaily
    // (resting HR, HRV, respiratory rate, skin temp, SpO2), keyed per day.
    const exported = await core.request<{ body?: Record<string, unknown> }>(
      "metrics.export_curated",
      { database_path: dbPath, source: "server_parse" },
    )
    let ingest: Awaited<ReturnType<typeof ingestMetrics>> | undefined
    if (exported.body) {
      const push = metricsPushSchema.parse(exported.body)
      ingest = await ingestMetrics(db, userId, push)
    }

    // Read-time scores (recovery/strain/stress), computed the same way the
    // device does, attributed to the most recent day with data. Sequential
    // calls — the sidecar handles one stdio request at a time.
    const recoveryReport = await core.request<Record<string, unknown>>(
      "metrics.recovery_score_from_features",
      { database_path: dbPath, ...SCORE_ARGS },
    )
    const strainReport = await core.request<Record<string, unknown>>(
      "metrics.strain_score_from_features",
      { database_path: dbPath, ...SCORE_ARGS },
    )
    const stressReport = await core.request<Record<string, unknown>>(
      "metrics.stress_score_from_features",
      { database_path: dbPath, ...SCORE_ARGS },
    )
    const day = latestExportDay(exported.body)
    if (day) {
      const recoveryScore = scoreValue(recoveryReport)
      const strainScore = scoreValue(strainReport)
      const stressScore = scoreValue(stressReport)
      const vitalsForDay = (exported.body?.vitals as Array<Record<string, unknown>> | undefined)
        ?.find((row) => row.day === day)
      // Store the full report in each family's `raw` so the app loads it into
      // packetScoreReports verbatim (same shape it used to compute on-device).
      // Reports are pushed even when a score is null so the app always has all
      // four (e.g. to render honest "why unavailable" states).
      const scorePush = metricsPushSchema.parse({
        source: "server_parse",
        recovery: [
          {
            day,
            recovery_score: recoveryScore,
            hrv_ms: (vitalsForDay?.hrv_ms as number | null | undefined) ?? null,
            resting_hr_bpm: (vitalsForDay?.resting_hr_bpm as number | null | undefined) ?? null,
            raw: recoveryReport,
          },
        ],
        strain: [{ day, strain_score: strainScore, raw: strainReport }],
        stress: [{ day, stress_score: stressScore, raw: stressReport }],
      })
      await ingestMetrics(db, userId, scorePush)

      // export_curated wrote daily_sleep.raw as the rollup row; replace it with
      // the full sleep score report. Attribute it to the latest *sleep* day (a
      // night maps to its own day, which can lag the latest vitals/recovery day)
      // so the row the app fetches (newest sleep) carries the report.
      const sleepDays = (exported.body?.sleep as Array<{ day?: unknown }> | undefined)
        ?.map((row) => row.day)
        .filter((value): value is string => typeof value === "string") ?? []
      const latestSleepDay = sleepDays.length > 0 ? sleepDays.sort().at(-1)! : day
      await db
        .update(dailySleep)
        .set({ raw: sleepReport })
        .where(and(eq(dailySleep.userId, userId), eq(dailySleep.day, latestSleepDay)))
    }

    await db
      .update(uploadBundles)
      .set({ status: "parsed", parsedAt: new Date(), parseError: null })
      .where(eq(uploadBundles.id, bundleId))

    return {
      bundleId,
      frameCount: frames.length,
      status: "parsed",
      ...(pipeline.reports !== undefined ? { reports: pipeline.reports } : {}),
      ...(ingest !== undefined ? { ingested: ingest } : {}),
    }
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error)
    await db
      .update(uploadBundles)
      .set({ status: "failed", parseError: message })
      .where(eq(uploadBundles.id, bundleId))
    return { bundleId, frameCount: 0, status: "failed", error: message }
  } finally {
    core.close()
  }
}
