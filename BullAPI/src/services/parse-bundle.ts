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
import { and, eq, inArray } from "drizzle-orm"
import type { Db } from "../db/client.ts"
import { dailySleep, inputReports, uploadBundles } from "../db/schema.ts"
import { getBundleForUser } from "./data-read.ts"
import { computeInputReports } from "./input-reports.ts"
import { ingestMetrics, metricsPushSchema } from "./metrics-ingest.ts"
import { getUserProfile } from "./profile.ts"
import { sendRecoveryPush } from "./apns.ts"
import { BullCore } from "../lib/bull-core.ts"
import type { Env } from "../lib/env.ts"
import type { ObjectStore } from "../lib/object-store.ts"

const DEVICE_MODEL = "WHOOP 5.0 Bull"
const LOCAL_BIOMETRIC_DEVICE_ID = "bull-local"
// Rolling window of raw frames the per-user compute store keeps. Once a day's
// results are folded into the durable Postgres tables, the workspace only needs
// recent history to compute current scores/baselines — pruning older frames
// keeps every compute cheap regardless of total history. Frames are pruned by
// captured_at (receive time), which is how the score engine windows them.
const STORE_RETENTION_DAYS = 60

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
  /** When present and APNs is configured, recovery computes emit a push. */
  env?: Env
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

export function deviceStorePath(dataDir: string, userId: string): string {
  return `${dataDir.replace(/\/$/, "")}/${userId}.sqlite`
}

/** Fetch a bundle from object storage and import its frames into the user's
 * store. No compute — the expensive pipeline/score scans run once per drain
 * cycle (computeUserStore), not per bundle. Returns the frame count. */
async function importBundleFrames(
  store: ObjectStore,
  core: BullCore,
  dbPath: string,
  storageKey: string,
): Promise<number> {
  const compressed = await store.get(storageKey)
  const jsonl = inflateRawSync(compressed).toString("utf8")
  const frames: BundleFrameLine[] = jsonl
    .split("\n")
    .filter((line) => line.trim().length > 0)
    .map((line) => JSON.parse(line) as BundleFrameLine)
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
  return frames.length
}

/** Run the full compute (pipeline + sleep/recovery/strain/stress scores) over a
 * user's store and fold the results into Postgres. This scans the whole store,
 * so it must run ONCE per drain cycle, not per bundle. */
async function computeUserStore(
  db: Db,
  core: BullCore,
  userId: string,
  dbPath: string,
  config?: ParseBundleConfig,
): Promise<void> {
  const windows = pipelineWindows(new Date())
  await core.request("metrics.run_pipeline", {
    database_path: dbPath,
    device_id: LOCAL_BIOMETRIC_DEVICE_ID,
    daily_window: windows.daily,
    hourly_window: windows.hourly,
  })
  const sleepReport = await core.request<Record<string, unknown>>(
    "metrics.sleep_score_from_features",
    { database_path: dbPath, ...SLEEP_SCORE_ARGS },
  )
  const exported = await core.request<{ body?: Record<string, unknown> }>(
    "metrics.export_curated",
    { database_path: dbPath, source: "server_parse" },
  )
  if (exported.body) {
    await ingestMetrics(db, userId, metricsPushSchema.parse(exported.body))
  }
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
  if (!day) return
  const vitalsForDay = (exported.body?.vitals as Array<Record<string, unknown>> | undefined)
    ?.find((row) => row.day === day)
  await ingestMetrics(
    db,
    userId,
    metricsPushSchema.parse({
      source: "server_parse",
      recovery: [
        {
          day,
          recovery_score: scoreValue(recoveryReport),
          hrv_ms: (vitalsForDay?.hrv_ms as number | null | undefined) ?? null,
          resting_hr_bpm: (vitalsForDay?.resting_hr_bpm as number | null | undefined) ?? null,
          raw: recoveryReport,
        },
      ],
      strain: [{ day, strain_score: scoreValue(strainReport), raw: strainReport }],
      stress: [{ day, stress_score: scoreValue(stressReport), raw: stressReport }],
    }),
  )
  // Notify the user's devices that a fresh recovery score is available. Fire-
  // and-forget + de-duped per (user, day); never blocks or fails the parse.
  if (config?.env) {
    try {
      await sendRecoveryPush(db, config.env, userId, day, scoreValue(recoveryReport))
    } catch {
      // best-effort: a push failure must not fail the parse
    }
  }
  const sleepDays = (exported.body?.sleep as Array<{ day?: unknown }> | undefined)
    ?.map((row) => row.day)
    .filter((value): value is string => typeof value === "string") ?? []
  const latestSleepDay = sleepDays.length > 0 ? sleepDays.sort().at(-1)! : day
  await db
    .update(dailySleep)
    .set({ raw: sleepReport })
    .where(and(eq(dailySleep.userId, userId), eq(dailySleep.day, latestSleepDay)))

  // Packet-derived input reports (HRV, resting HR, steps, energy, motion, vital
  // events, daily/hourly rollups) — the map the app reads to render dashboards.
  // One latest row per user, computed over the whole store. The user's profile
  // (weight/age/sex/timezone) drives energy estimates + local-day bucketing.
  const profile = await getUserProfile(db, userId)
  const inputReportsMap = await computeInputReports(core, dbPath, { profile })
  await db
    .insert(inputReports)
    .values({ userId, raw: inputReportsMap })
    .onConflictDoUpdate({
      target: inputReports.userId,
      set: { raw: inputReportsMap, computedAt: new Date() },
    })

  // Bound the workspace: results are now durable in Postgres, so drop raw frames
  // older than the baseline window. Keeps each compute's full-store scan cheap.
  const cutoff = new Date(Date.now() - STORE_RETENTION_DAYS * 86_400_000).toISOString()
  await core.request("store.prune_raw_evidence_before", {
    database_path: dbPath,
    captured_before: cutoff,
  })
}

// Serializes all drains (background interval + per-upload trigger) so only one
// runs at a time — prevents concurrent writers to the same per-user SQLite store
// and redundant re-compute. Caps drain CPU at a single core.
let draining = false
// Compute (run_pipeline + score scans) re-reads the WHOLE per-user store, so it
// is expensive and grows with history. Throttle it: at most one drain that does
// real work per this interval, regardless of how often uploads arrive. New data
// is therefore at most this stale — fine for daily scores, and it keeps CPU from
// pegging on a trickle of uploads.
let lastDrainAt = 0
const DRAIN_INTERVAL_MS = 5 * 60_000

/**
 * Drain pending bundles across ALL users (bounded, oldest first). Imports every
 * bundle's frames, then runs compute ONCE per user — so a backlog of tiny
 * bundles costs N cheap imports + 1 compute, not N full pipelines. Returns the
 * number of bundles marked parsed.
 */
export async function parseAllPending(
  db: Db,
  store: ObjectStore,
  config: ParseBundleConfig,
  limit = 200,
): Promise<number> {
  if (draining) return 0
  if (Date.now() - lastDrainAt < DRAIN_INTERVAL_MS) return 0
  draining = true
  try {
    const pending = await db
      .select({
        id: uploadBundles.id,
        userId: uploadBundles.userId,
        storageKey: uploadBundles.storageKey,
      })
      .from(uploadBundles)
      .where(eq(uploadBundles.status, "pending"))
      .orderBy(uploadBundles.createdAt)
      .limit(limit)
    // Only consume the throttle window when there is actual work, so an idle
    // poll doesn't delay the next real drain.
    if (pending.length === 0) return 0
    lastDrainAt = Date.now()

    const byUser = new Map<string, { id: string; storageKey: string }[]>()
    for (const bundle of pending) {
      const list = byUser.get(bundle.userId) ?? []
      list.push({ id: bundle.id, storageKey: bundle.storageKey })
      byUser.set(bundle.userId, list)
    }

    let parsed = 0
    for (const [userId, bundles] of byUser) {
      let core = new BullCore(config.binaryPath)
      const dbPath = deviceStorePath(config.dataDir, userId)
      const importedIds: string[] = []
      try {
        for (const bundle of bundles) {
          try {
            await importBundleFrames(store, core, dbPath, bundle.storageKey)
            importedIds.push(bundle.id)
          } catch (error) {
            const message = errorMessage(error)
            await db
              .update(uploadBundles)
              .set({ status: "failed", parseError: message })
              .where(eq(uploadBundles.id, bundle.id))
            // If a bundle hard-crashed the sidecar (segfault/abort that
            // catch_unwind can't intercept), the process is now dead and every
            // remaining import in this batch would cascade-fail with the same
            // "closed unexpectedly". Respawn a fresh sidecar so one poison
            // bundle costs one failure, not the whole batch.
            if (sidecarDied(message)) {
              core.close()
              core = new BullCore(config.binaryPath)
            }
          }
        }
        if (importedIds.length > 0) {
          await computeUserStore(db, core, userId, dbPath, config)
          await db
            .update(uploadBundles)
            .set({ status: "parsed", parsedAt: new Date(), parseError: null })
            .where(inArray(uploadBundles.id, importedIds))
          parsed += importedIds.length
        }
      } catch (error) {
        // Compute failed: the frames are already imported (and feed future
        // computes), so mark these failed rather than retry-looping on bad data.
        if (importedIds.length > 0) {
          await db
            .update(uploadBundles)
            .set({ status: "failed", parseError: errorMessage(error) })
            .where(inArray(uploadBundles.id, importedIds))
        }
      } finally {
        core.close()
      }
    }
    return parsed
  } finally {
    draining = false
  }
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}

/** True when an error means the sidecar process itself died (EOF on stdout),
 * as opposed to a structured parse error the sidecar returned while alive. */
function sidecarDied(message: string): boolean {
  return message.includes("closed unexpectedly")
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
    const dbPath = deviceStorePath(config.dataDir, userId)
    const frameCount = await importBundleFrames(store, core, dbPath, bundle.storageKey)
    await computeUserStore(db, core, userId, dbPath, config)
    await db
      .update(uploadBundles)
      .set({ status: "parsed", parsedAt: new Date(), parseError: null })
      .where(eq(uploadBundles.id, bundleId))
    return { bundleId, frameCount, status: "parsed" }
  } catch (error) {
    const message = errorMessage(error)
    await db
      .update(uploadBundles)
      .set({ status: "failed", parseError: message })
      .where(eq(uploadBundles.id, bundleId))
    return { bundleId, frameCount: 0, status: "failed", error: message }
  } finally {
    core.close()
  }
}
