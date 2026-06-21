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
//
// This MUST stay small: a WHOOP 5.0 streams per-second IMU/HR, so each retained
// day is ~150-200 MB once the compute materialises it. The feature passes load
// the whole retained window into memory, so retention is effectively the
// compute's peak-memory knob. Long-term baselines (EWMA, daily rollups) persist
// in their own tables and survive the prune, so a short raw-frame window does
// not lose baseline history. Override per-deployment via BULL_STORE_RETENTION_DAYS.
const STORE_RETENTION_DAYS = Math.max(
  1,
  Number(process.env.BULL_STORE_RETENTION_DAYS ?? "3") || 3,
)

// Nightly sleep-score tuning. Mirrors the device's read-time score call
// (HealthDataStore+Utilities.swift `sleepScoreReport`); kept here so the server
// produces the same per-night scores. "0000"/"9999" are full-range scan bounds
// (not timestamps), so no window derivation is needed.
function sleepScoreArgs() {
  const start = new Date(Date.now() - 30 * 86_400_000).toISOString()
  return {
  start,
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
}
}

// ponytail: bound scans to 30 days back from today to keep sidecar calls fast
// on large stores. Full-range "0000"→"9999" caused sidecar timeouts.
function scoreArgs() {
  const start = new Date(Date.now() - 30 * 86_400_000).toISOString()
  return {
  start,
  end: "9999",
  min_owned_captures: 2,
  require_trusted_evidence: false,
  hrv_start: start,
  hrv_end: "9999",
  hrv_baseline_start: start,
  hrv_baseline_end: "9999",
  resting_start: start,
  resting_end: "9999",
  sleep_start: start,
  sleep_end: "9999",
  prior_strain_start: start,
  prior_strain_end: "9999",
  resting_baseline_min_days: 3,
  hrv_min_rr_intervals_to_compute: 2,
  hrv_baseline_min_days: 3,
  sleep_need_minutes: 480.0,
  low_motion_threshold_0_to_1: 0.05,
  disturbance_motion_threshold_0_to_1: 0.2,
  target_midpoint_minutes_since_midnight: 180.0,
  prior_strain_resting_baseline_min_days: 3,
}
}

/** Pull the 0-100 score out of a *_score_from_features report. */
function scoreValue(report: unknown): number | null {
  const output = (report as { score_result?: { output?: Record<string, unknown> } })
    ?.score_result?.output
  if (!output) return null
  // strain uses score_0_to_21; recovery/sleep/stress use score_0_to_100
  for (const key of ["score_0_to_100", "score_0_to_21"]) {
    if (typeof output[key] === "number") return output[key] as number
  }
  return null
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
): Promise<{ frameCount: number; days: Set<string> }> {
  const compressed = await store.get(storageKey)
  const jsonl = inflateRawSync(compressed).toString("utf8")
  const frames: BundleFrameLine[] = jsonl
    .split("\n")
    .filter((line) => line.trim().length > 0)
    .map((line) => JSON.parse(line) as BundleFrameLine)
  const days = new Set<string>()
  const importFrames = frames.map((frame) => {
    if (frame.captured_at) days.add(frame.captured_at.slice(0, 10))
    return {
      evidence_id: frame.evidence_id,
      source: "server.parse",
      captured_at: frame.captured_at,
      device_model: DEVICE_MODEL,
      frame_hex: frame.payload_hex,
      sensitivity: "user-owned-capture",
      device_type: "BULL" as const,
    }
  })
  await core.request("capture.import_frame_batch", {
    database_path: dbPath,
    frames: importFrames,
    include_timeline_rows: false,
    include_results: false,
  })
  return { frameCount: frames.length, days }
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
  dataDays?: Set<string>,
): Promise<void> {
  // Prune BEFORE compute, not after. The feature passes materialise the whole
  // retained window into memory, so an unbounded store OOM-kills run_pipeline
  // (SIGKILL) before it can reach the old prune-at-the-end. Bounding the window
  // up front caps peak memory at the retention window and breaks the vicious
  // cycle where a crash left the store to grow forever.
  const cutoff = new Date(Date.now() - STORE_RETENTION_DAYS * 86_400_000).toISOString()
  await core.request("store.prune_raw_evidence_before", {
    database_path: dbPath,
    captured_before: cutoff,
  })

  // Run the pipeline for each day that has imported data + today.
  // Each run only does the rollup/write step for that day; the feature passes
  // (HR, HRV, motion etc.) scan the full store regardless of the daily window.
  const today = new Date()
  const todayKey = today.toISOString().slice(0, 10)
  const dayKeys = new Set<string>([todayKey])
  if (dataDays) for (const d of dataDays) dayKeys.add(d)
  // ponytail: cap at 7 days max. Each run_pipeline does a full-store scan.
  // 7 is enough to cover a week of buffered band syncs without OOM risk.
  const sortedDays = [...dayKeys].sort().slice(-7)
  console.log(`[compute] ${userId} running pipeline for days: ${sortedDays.join(", ")}`)
  for (const k of sortedDays) {
    const windows = pipelineWindows(new Date(k + "T00:00:00Z"))
    await core.request("metrics.run_pipeline", {
      database_path: dbPath,
      device_id: LOCAL_BIOMETRIC_DEVICE_ID,
      daily_window: windows.daily,
      hourly_window: windows.hourly,
    })
  }

  const sleepReport = await core.request<Record<string, unknown>>(
    "metrics.sleep_score_from_features",
    { database_path: dbPath, ...sleepScoreArgs() },
  )
  const exported = await core.request<{ body?: Record<string, unknown> }>(
    "metrics.export_curated",
    { database_path: dbPath, source: "server_parse" },
  )
  // Merge vitals entries for the same day (export_curated returns one row per
  // metric_id, so a single day may have separate resting_hr and hrv rows).
  const rawVitals = (exported.body?.vitals ?? []) as Array<Record<string, unknown>>
  const vitalsByDay = new Map<string, Record<string, unknown>>()
  for (const v of rawVitals) {
    const day = v.day as string | undefined
    if (!day) continue
    const existing = vitalsByDay.get(day) ?? { day }
    for (const [k, val] of Object.entries(v)) {
      if (val != null && k !== "raw") existing[k] = val
    }
    vitalsByDay.set(day, existing)
  }
  const vitalsArray = [...vitalsByDay.values()]

  // Ingest with merged vitals so no null clobbering occurs.
  if (exported.body) {
    const mergedBody = { ...exported.body, vitals: vitalsArray }
    await ingestMetrics(db, userId, metricsPushSchema.parse(mergedBody))
  }
  for (const vitalsForDay of vitalsArray) {
    const day = vitalsForDay?.day as string | undefined
    if (!day) continue
    const dayScoreArgs = { database_path: dbPath, ...scoreArgs(), date_key: day }
    const recoveryReport = await core.request<Record<string, unknown>>(
      "metrics.recovery_score_from_features",
      dayScoreArgs,
    )
    const strainReport = await core.request<Record<string, unknown>>(
      "metrics.strain_score_from_features",
      dayScoreArgs,
    )
    const stressReport = await core.request<Record<string, unknown>>(
      "metrics.stress_score_from_features",
      dayScoreArgs,
    )
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
    // Notify the user's devices that a fresh recovery score is available.
    if (config?.env) {
      try {
        await sendRecoveryPush(db, config.env, userId, day, scoreValue(recoveryReport))
      } catch {
        // best-effort: a push failure must not fail the parse
      }
    }
  } // end per-day loop

  const latestDay = vitalsArray.map((v) => v.day as string).filter(Boolean).sort().at(-1)
  const sleepDays = (exported.body?.sleep as Array<{ day?: unknown }> | undefined)
    ?.map((row) => row.day)
    .filter((value): value is string => typeof value === "string") ?? []
  const latestSleepDay = sleepDays.length > 0 ? sleepDays.sort().at(-1)! : latestDay ?? new Date().toISOString().slice(0, 10)
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

  // (Raw-frame pruning now runs at the START of this function so the heavy
  // feature passes never scan an unbounded window.)
}

// Serializes import+compute drains so only one runs at a time — prevents
// concurrent writers to the same per-user SQLite store.
let draining = false

/** Steady-state throughput: imports stay aggressive; full compute is debounced per user. */
function drainImportBatchSize(): number {
  return Math.max(50, Math.min(1000, Number(process.env.BULL_DRAIN_IMPORT_BATCH ?? "500") || 500))
}

function computeMinIntervalMs(): number {
  return Math.max(
    60_000,
    Math.min(600_000, Number(process.env.BULL_COMPUTE_MIN_INTERVAL_MS ?? "180000") || 180_000),
  )
}

const lastComputeAtByUser = new Map<string, number>()

export interface ParseDrainResult {
  imported: number
  computedUsers: string[]
  error?: string
}

/** Force compute on next drain for this user (ignores compute debounce once). */
export function requestParseComputeForUser(userId: string): void {
  lastComputeAtByUser.delete(userId)
}

function shouldRunCompute(userId: string, forceCompute: boolean): boolean {
  if (forceCompute) return true
  const last = lastComputeAtByUser.get(userId) ?? 0
  return Date.now() - last >= computeMinIntervalMs()
}

type ParseDrainOptions = { limit?: number; forceCompute?: boolean; forceComputeUserId?: string }

/** One import batch + optional debounced compute. Caller must hold `draining` if serializing. */
async function importAndComputeBatch(
  db: Db,
  store: ObjectStore,
  config: ParseBundleConfig,
  options?: ParseDrainOptions,
): Promise<ParseDrainResult> {
  const empty: ParseDrainResult = { imported: 0, computedUsers: [] }
  const limit = options?.limit ?? drainImportBatchSize()
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
    if (pending.length === 0) {
      // No imports but caller wants compute for a specific user anyway.
      if (options?.forceComputeUserId && options?.forceCompute) {
        const userId = options.forceComputeUserId
        const core = new BullCore(config.binaryPath)
        const dbPath = deviceStorePath(config.dataDir, userId)
        try {
          await computeUserStore(db, core, userId, dbPath, config, new Set())
          lastComputeAtByUser.set(userId, Date.now())
          return { imported: 0, computedUsers: [userId] }
        } catch (error) {
          const msg = errorMessage(error)
          console.error(`[parse] forced compute failed for ${userId}: ${msg}`)
          // Write error to a bundle so we can read it from DB
          try {
            await db
              .update(uploadBundles)
              .set({ parseError: `COMPUTE_ERROR: ${msg.slice(0, 500)}` })
              .where(sql`${uploadBundles.userId} = ${userId} AND ${uploadBundles.id} = (
                SELECT id FROM upload_bundles WHERE user_id = ${userId} ORDER BY created_at DESC LIMIT 1
              )`)
          } catch {}
          return { imported: 0, computedUsers: [], error: msg }
        } finally {
          core.close()
        }
      }
      return empty
    }

    const byUser = new Map<string, { id: string; storageKey: string }[]>()
    for (const bundle of pending) {
      const list = byUser.get(bundle.userId) ?? []
      list.push({ id: bundle.id, storageKey: bundle.storageKey })
      byUser.set(bundle.userId, list)
    }

    const dirtyUsers = new Map<string, Set<string>>()
    let imported = 0

    for (const [userId, bundles] of byUser) {
      let core = new BullCore(config.binaryPath)
      const dbPath = deviceStorePath(config.dataDir, userId)
      const importedIds: string[] = []
      const importedDays = new Set<string>()
      try {
        for (const bundle of bundles) {
          try {
            const { days } = await importBundleFrames(store, core, dbPath, bundle.storageKey)
            for (const d of days) importedDays.add(d)
            importedIds.push(bundle.id)
          } catch (error) {
            const message = errorMessage(error)
            await db
              .update(uploadBundles)
              .set({ status: "failed", parseError: message })
              .where(eq(uploadBundles.id, bundle.id))
            if (sidecarDied(message)) {
              core.close()
              core = new BullCore(config.binaryPath)
            }
          }
        }
        if (importedIds.length > 0) {
          await db
            .update(uploadBundles)
            .set({ status: "parsed", parsedAt: new Date(), parseError: null })
            .where(inArray(uploadBundles.id, importedIds))
          imported += importedIds.length
          dirtyUsers.set(userId, importedDays)
        }
      } finally {
        core.close()
      }
    }

    const computedUsers: string[] = []
    const forceCompute = options?.forceCompute === true
    for (const [userId, importedDays] of dirtyUsers) {
      if (!shouldRunCompute(userId, forceCompute)) continue
      const core = new BullCore(config.binaryPath)
      const dbPath = deviceStorePath(config.dataDir, userId)
      try {
        await computeUserStore(db, core, userId, dbPath, config, importedDays)
        lastComputeAtByUser.set(userId, Date.now())
        computedUsers.push(userId)
      } catch (error) {
        const msg = errorMessage(error)
        console.error(`[parse] COMPUTE FAILED for ${userId}: ${msg}`)
        // Write error to DB for diagnostic visibility
        try {
          await db
            .update(uploadBundles)
            .set({ parseError: `COMPUTE_ERROR: ${msg.slice(0, 500)}` })
            .where(sql`${uploadBundles.userId} = ${userId} AND ${uploadBundles.id} = (
              SELECT id FROM upload_bundles WHERE user_id = ${userId} ORDER BY created_at DESC LIMIT 1
            )`)
        } catch {}
        // Set debounce even on failure so the drain loop doesn't retry
        // compute 40 times in the same wake. Next wake retries.
        lastComputeAtByUser.set(userId, Date.now())
      } finally {
        core.close()
      }
    }

    return { imported, computedUsers }
}

/**
 * Import until pending is empty or maxBatches (no idle cooldown between batches).
 * Only `draining` + per-user compute debounce limit concurrency / CPU.
 */
export async function runParseDrainLoop(
  db: Db,
  store: ObjectStore,
  config: ParseBundleConfig,
  options?: ParseDrainOptions & { maxBatches?: number },
): Promise<ParseDrainResult> {
  const empty: ParseDrainResult = { imported: 0, computedUsers: [] }
  if (draining) return empty
  draining = true
  try {
    const maxBatches = Math.max(1, Math.min(100, options?.maxBatches ?? 40))
    let totalImported = 0
    const computed = new Set<string>()
    for (let i = 0; i < maxBatches; i++) {
      const batch = await importAndComputeBatch(db, store, config, options)
      totalImported += batch.imported
      for (const u of batch.computedUsers) computed.add(u)
      if (batch.imported === 0) break
    }
    return { imported: totalImported, computedUsers: [...computed] }
  } finally {
    draining = false
  }
}

/** Single batch (e.g. fire-and-forget after one upload). */
export async function parseAllPending(
  db: Db,
  store: ObjectStore,
  config: ParseBundleConfig,
  options?: ParseDrainOptions,
): Promise<ParseDrainResult> {
  return runParseDrainLoop(db, store, config, { ...options, maxBatches: 1 })
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
    const { frameCount, days } = await importBundleFrames(store, core, dbPath, bundle.storageKey)
    await computeUserStore(db, core, userId, dbPath, config, days)
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
