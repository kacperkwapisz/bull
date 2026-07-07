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
import { and, desc, eq, gte, inArray, isNotNull, lt, or, sql } from "drizzle-orm"
import type { Db } from "../db/client.ts"
import { dailyRecovery, dailySleep, dailyStrain, inputReports, uploadBundles } from "../db/schema.ts"
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
// Rolling window of raw frames the per-user compute store keeps *after a
// successful projection*. The raw upload bundle remains the source of record,
// but the local compute store must not prune newly imported evidence until the
// affected days have been folded into durable Postgres projections. Otherwise a
// repeated compute failure can erase unprojected frames from the hot store and
// make the app-facing tables stale until a full object-storage rebuild.
//
// Frames are pruned by captured_at (receive time), which is how the score engine
// windows them. This still MUST stay small once compute succeeds: device streams
// are dense, and feature passes load the retained window into memory. Override
// per-deployment via BULL_STORE_RETENTION_DAYS.
const STORE_RETENTION_DAYS = Math.max(
  1,
  Number(process.env.BULL_STORE_RETENTION_DAYS ?? "5") || 5,
)

// Server hot-store maintenance caps. Raw bundles in object storage are the
// source of record, so SQLite keeps only a bounded recent payload cache while
// retaining decoded rows, parsed payload JSON, and all metric tables.
const SERVER_STORE_RAW_PAYLOAD_LIMIT_BYTES = 64 * 1024 * 1024
const SERVER_STORE_DECODED_PAYLOAD_LIMIT_BYTES = 64 * 1024 * 1024
const SERVER_STORE_VACUUM_MIN_FREE_BYTES = 64 * 1024 * 1024
const SERVER_STORE_VACUUM_MIN_FREE_PERCENT = 10

// Plausible main-sleep duration band, mirroring the Rust nightly gate
// (`MIN/MAX_MAIN_SLEEP_MINUTES` in bridge.rs). Used to purge physiologically
// impossible sleep rows from the durable projection. No real night is shorter
// than 3h or longer than 14h, so this is timezone-independent.
const MIN_MAIN_SLEEP_MINUTES = 180
const MAX_MAIN_SLEEP_MINUTES = 840

// Nightly sleep-score tuning. Mirrors the device's read-time score call
// (HealthDataStore+Utilities.swift `sleepScoreReport`); kept here so the server
// produces the same per-night scores. "0000"/"9999" are full-range scan bounds
// (not timestamps), so no window derivation is needed.
function sleepScoreArgs() {
  const start = new Date(Date.now() - 14 * 86_400_000).toISOString()
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
  const start = new Date(Date.now() - 14 * 86_400_000).toISOString()
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

/** The user's UTC offset in minutes for a given instant, derived from their
 * uploaded IANA timezone (e.g. "Europe/Warsaw"). DST-correct because it asks
 * Intl for the offset at that specific instant. Returns null when the timezone
 * is absent or unrecognized — callers must then skip local-time gating rather
 * than assume a zone. */
function utcOffsetMinutesForInstant(timezone: string | null, atUtcMs: number): number | null {
  if (!timezone) return null
  try {
    const dtf = new Intl.DateTimeFormat("en-US", {
      timeZone: timezone,
      hour12: false,
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    })
    const parts = dtf.formatToParts(new Date(atUtcMs))
    const get = (t: string) => Number(parts.find((p) => p.type === t)?.value)
    const asUtc = Date.UTC(
      get("year"),
      get("month") - 1,
      get("day"),
      get("hour") === 24 ? 0 : get("hour"),
      get("minute"),
      get("second"),
    )
    if (!Number.isFinite(asUtc)) return null
    return Math.round((asUtc - atUtcMs) / 60_000)
  } catch {
    return null
  }
}

/** Retention cutoff for hot-store prune (same watermark as post-compute prune). */
function storeRetentionCutoffIso(): string {
  return new Date(Date.now() - STORE_RETENTION_DAYS * 86_400_000).toISOString()
}

/** Trim the per-user SQLite hot store before compute. Raw upload bundles remain
 * the source of record in object storage; pruning here prevents unbounded growth
 * when compute fails and post-success prune never runs. */
async function pruneHotStoreRetentionWindow(
  core: BullCore,
  userId: string,
  dbPath: string,
): Promise<void> {
  const cutoff = storeRetentionCutoffIso()
  try {
    await core.request("store.prune_raw_evidence_before", {
      database_path: dbPath,
      captured_before: cutoff,
    })
    console.log(`[compute] ${userId} pre-compute prune before ${cutoff}`)
  } catch (error) {
    console.error(`[compute] ${userId} pre-compute prune failed: ${errorMessage(error)}`)
  }
}

type StoreMaintenancePayloadReport = {
  compacted_rows?: number
}

type StoreMaintenanceReport = {
  file_bytes_before?: number
  file_bytes_after?: number
  vacuumed?: boolean
  wal_bytes_before?: number
  wal_bytes_after?: number
  wal_checkpoint_busy?: boolean
  raw_evidence?: StoreMaintenancePayloadReport
  decoded_frames?: StoreMaintenancePayloadReport
}

function megabytes(bytes: number | undefined): string {
  return `${((bytes ?? 0) / 1_000_000).toFixed(1)}MB`
}

async function maintainHotStore(core: BullCore, userId: string, dbPath: string): Promise<void> {
  try {
    const report = await core.request<StoreMaintenanceReport>("store.maintain", {
      database_path: dbPath,
      raw_payload_limit_bytes: SERVER_STORE_RAW_PAYLOAD_LIMIT_BYTES,
      decoded_payload_limit_bytes: SERVER_STORE_DECODED_PAYLOAD_LIMIT_BYTES,
      vacuum_min_free_bytes: SERVER_STORE_VACUUM_MIN_FREE_BYTES,
      vacuum_min_free_percent: SERVER_STORE_VACUUM_MIN_FREE_PERCENT,
    })
    console.log(
      `[compute] ${userId} store maintenance: db=${megabytes(report.file_bytes_before)}->${megabytes(report.file_bytes_after)} vacuumed=${report.vacuumed === true} raw_compacted=${report.raw_evidence?.compacted_rows ?? 0} decoded_compacted=${report.decoded_frames?.compacted_rows ?? 0} wal=${megabytes(report.wal_bytes_before)}->${megabytes(report.wal_bytes_after)} busy=${report.wal_checkpoint_busy === true}`,
    )
  } catch (error) {
    console.error(`[compute] ${userId} store maintenance failed: ${errorMessage(error)}`)
  }
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
  try {
    await computeUserStoreProjections(db, core, userId, dbPath, config, dataDays)
  } finally {
    await maintainHotStore(core, userId, dbPath)
  }
}

async function computeUserStoreProjections(
  db: Db,
  core: BullCore,
  userId: string,
  dbPath: string,
  config?: ParseBundleConfig,
  dataDays?: Set<string>,
): Promise<void> {
  // Run the pipeline for each day that has imported data + a recent catch-up
  // window. The catch-up window is important after a compute outage: bundles may
  // already be imported/parsed, so there are no pending rows to tell us which
  // days still need projections. Re-running recent days is idempotent and keeps
  // app-facing tables current without dropping any raw evidence.
  const today = new Date()
  const todayKey = today.toISOString().slice(0, 10)
  const dayKeys = new Set<string>([todayKey])
  for (let i = 1; i <= STORE_RETENTION_DAYS; i += 1) {
    dayKeys.add(new Date(Date.now() - i * 86_400_000).toISOString().slice(0, 10))
  }
  if (dataDays) for (const d of dataDays) dayKeys.add(d)

  const sortedDays = [...dayKeys].sort()

  await pruneHotStoreRetentionWindow(core, userId, dbPath)

  const runPipelineDay = async (day: string, skipFeaturePasses: boolean) => {
    const windows = pipelineWindows(new Date(day + "T00:00:00Z"))
    const featureWindowStart = new Date(
      windows.daily.start_time_unix_ms - STORE_RETENTION_DAYS * 86_400_000,
    ).toISOString()
    await core.request("metrics.run_pipeline", {
      database_path: dbPath,
      device_id: LOCAL_BIOMETRIC_DEVICE_ID,
      daily_window: windows.daily,
      hourly_window: windows.hourly,
      feature_window_start_iso: featureWindowStart,
      skip_feature_passes: skipFeaturePasses,
      // Discovery pass is validation-only and loads the full decoded window.
      skip_step_discovery: true,
    })
  }

  console.log(`[compute] ${userId} running pipeline rollups for ${todayKey}; backfill days: ${sortedDays.join(", ")}`)
  try {
    // The dense feature-pass block still has full-window reports that can exceed
    // the 2 GB production budget on retained multi-day stores. Nightly sleep is
    // computed below in scoped noon-to-noon windows, and the rollup path remains
    // safe, so server parse skips feature passes here until all reports stream.
    await runPipelineDay(todayKey, true)
  } catch (error) {
    console.error(
      `[compute] ${userId} pipeline rollups failed for ${todayKey}: ${errorMessage(error)}`,
    )
  }
  for (const k of sortedDays) {
    if (k === todayKey) continue
    try {
      await runPipelineDay(k, true)
    } catch (error) {
      console.error(`[compute] ${userId} pipeline rollups failed for ${k}: ${errorMessage(error)}`)
    }
  }

  // Persist nightly sleep per day. A single full-range scan only writes one
  // best window for the whole store (merging multiple nights into one blob).
  // Scoring each day in its own night-scoped, noon-to-noon window rebuilds the
  // full retention window with the current gates; each call replaces that day's
  // row in place (date-keyed persistence), so this is idempotent and does not
  // accumulate duplicates. Per-day windows are small, avoiding sidecar timeouts.
  // This relies on frames carrying real device timestamps (not upload time) so
  // each night's samples land in the correct noon-to-noon bucket.
  // The user's uploaded IANA timezone drives local-time night gating below
  // (no hardcoded offset). Absent timezone → the gate falls back to a
  // tz-independent duration check inside the scorer.
  const profile = await getUserProfile(db, userId)
  const userTimezone = profile?.timezone ?? null
  type NightlySleepScoreReport = {
    nightly_sleep_persisted?: boolean
    nightly_sleep_persist_reason?: string
    nightly_sleep_window?: {
      start?: string
      end?: string
      sleep_duration_minutes?: number
    }
  }

  let sleepPersistedDays = 0
  for (const day of sortedDays) {
    // A night "belongs to" the morning it ends on: scan from the prior midday
    // through this day's midday so the day's primary window is that night.
    const dayMs = new Date(day + "T00:00:00Z").getTime()
    const sleepStart = new Date(dayMs - 12 * 3_600_000).toISOString()
    const sleepEnd = new Date(dayMs + 12 * 3_600_000).toISOString()
    const offsetMinutes = utcOffsetMinutesForInstant(userTimezone, dayMs)
    const report = await core.request<NightlySleepScoreReport>(
      "metrics.sleep_score_from_features",
      {
        database_path: dbPath,
        ...sleepScoreArgs(),
        start: sleepStart,
        end: sleepEnd,
        ...(offsetMinutes != null ? { night_gate_utc_offset_minutes: offsetMinutes } : {}),
      },
    )
    const reason = report?.nightly_sleep_persist_reason ?? "unknown"
    const persisted = report?.nightly_sleep_persisted === true
    if (persisted) sleepPersistedDays += 1
    const win = report?.nightly_sleep_window
    const windowPart =
      win?.start != null && win?.end != null
        ? ` window=${win.start}..${win.end}${typeof win.sleep_duration_minutes === "number" ? ` dur=${win.sleep_duration_minutes}m` : ""}`
        : ""
    console.log(
      `[compute] ${userId} nightly sleep ${day}: ${persisted ? "persisted" : reason}${windowPart}`,
    )
  }
  console.log(`[compute] ${userId} nightly sleep persisted for ${sleepPersistedDays}/${sortedDays.length} days`)
  const exported = await core.request<{ body?: Record<string, unknown> }>(
    "metrics.export_curated",
    { database_path: dbPath, source: "server_parse" },
  )
  const exportCounts = exported.body
    ? { vitals: ((exported.body.vitals as any[]) ?? []).length, sleep: ((exported.body.sleep as any[]) ?? []).length }
    : { vitals: 0, sleep: 0 }
  console.log(`[compute] ${userId} export: vitals=${exportCounts.vitals} sleep=${exportCounts.sleep}`)
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
    // Per-day windows: HRV/RHR/strain scoped to the day; baselines use full window.
    const wideArgs = scoreArgs()
    const dayStart = day + "T00:00:00Z"
    const dayEnd = day + "T23:59:59Z"
    const recoveryArgs = {
      database_path: dbPath, ...wideArgs, date_key: day,
      // Day's HRV scoped to this day; resting HR and baselines stay wide
      hrv_start: dayStart, hrv_end: dayEnd,
      // Sleep: the night BEFORE this day
      sleep_start: new Date(new Date(dayStart).getTime() - 86_400_000).toISOString(),
      sleep_end: dayEnd,
      // Prior strain: the day before
      prior_strain_start: new Date(new Date(dayStart).getTime() - 86_400_000).toISOString(),
      prior_strain_end: dayStart,
    }
    const strainArgs = {
      database_path: dbPath, ...wideArgs, date_key: day,
      start: dayStart, end: dayEnd,
    }
    const recoveryReport = await core.request<Record<string, unknown>>(
      "metrics.recovery_score_from_features",
      recoveryArgs,
    )
    const strainReport = await core.request<Record<string, unknown>>(
      "metrics.strain_score_from_features",
      strainArgs,
    )
    const stressReport = await core.request<Record<string, unknown>>(
      "metrics.stress_score_from_features",
      { database_path: dbPath, ...wideArgs, date_key: day, start: dayStart, end: dayEnd },
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

  // Inject a `daily` array into the latest recovery row's raw so the app's
  // calibration check can count scored days without fetching multiple rows.
  {
    const scored = await db
      .select({ day: dailyRecovery.day, recovery_score: dailyRecovery.recoveryScore })
      .from(dailyRecovery)
      .where(and(eq(dailyRecovery.userId, userId), isNotNull(dailyRecovery.recoveryScore)))
      .orderBy(desc(dailyRecovery.day))
      .limit(14)
    if (scored.length > 0 && scored[0]) {
      const latest = scored[0]
      const existing = await db
        .select({ raw: dailyRecovery.raw })
        .from(dailyRecovery)
        .where(and(eq(dailyRecovery.userId, userId), eq(dailyRecovery.day, latest.day)))
        .limit(1)
      const rawObj = (existing[0]?.raw as Record<string, unknown>) ?? {}
      rawObj.daily = scored.map((r) => ({
        day: r.day,
        score_0_to_100: r.recovery_score,
      }))
      await db
        .update(dailyRecovery)
        .set({ raw: rawObj })
        .where(and(eq(dailyRecovery.userId, userId), eq(dailyRecovery.day, latest.day)))
    }
  }

  // Inject `daily` arrays into the latest sleep/strain rows' raw for app calibration.
  const latestDay = vitalsArray.map((v) => v.day as string).filter(Boolean).sort().at(-1)
  const sleepDays = (exported.body?.sleep as Array<{ day?: unknown }> | undefined)
    ?.map((row) => row.day)
    .filter((value): value is string => typeof value === "string") ?? []

  // Keep the durable projection honest WITHOUT deleting on transient absence.
  // Impossible-duration purge only: delete any row whose recorded sleep is
  // physiologically impossible for a main sleep (<3h or >14h), at any date
  // (including history whose raw frames are pruned and cannot be rescored). No
  // real night falls outside that band, so this is timezone-independent and
  // safe. We deliberately do NOT delete a day merely because the current
  // compute produced no window for it: during a full rebuild a cycle can
  // transiently emit sleep=0, and deleting on absence would erase real nights.
  {
    const impossible = await db
      .delete(dailySleep)
      .where(
        and(
          eq(dailySleep.userId, userId),
          or(
            lt(dailySleep.totalSleepMinutes, MIN_MAIN_SLEEP_MINUTES),
            gte(dailySleep.totalSleepMinutes, MAX_MAIN_SLEEP_MINUTES + 1),
          ),
        ),
      )
      .returning({ day: dailySleep.day })
    if (impossible.length > 0) {
      console.log(
        `[compute] ${userId} purged ${impossible.length} physiologically-impossible sleep row(s): ${impossible.map((r) => r.day).join(", ")}`,
      )
    }
  }
  const latestSleepDay = sleepDays.length > 0 ? sleepDays.sort().at(-1)! : latestDay ?? new Date().toISOString().slice(0, 10)
  {
    const scoredSleep = await db
      .select({ day: dailySleep.day, score: dailySleep.sleepScore })
      .from(dailySleep)
      .where(and(eq(dailySleep.userId, userId), isNotNull(dailySleep.sleepScore)))
      .orderBy(desc(dailySleep.day))
      .limit(14)
    const sleepRaw: Record<string, unknown> = {}
    sleepRaw.daily = scoredSleep.map((r) => ({
      day: r.day,
      score_0_to_100: r.score,
      sleep_duration_minutes: null,
    }))
    await db
      .update(dailySleep)
      .set({ raw: sleepRaw })
      .where(and(eq(dailySleep.userId, userId), eq(dailySleep.day, latestSleepDay)))
  }
  {
    const scoredStrain = await db
      .select({ day: dailyStrain.day, score: dailyStrain.strainScore })
      .from(dailyStrain)
      .where(and(eq(dailyStrain.userId, userId), isNotNull(dailyStrain.strainScore)))
      .orderBy(desc(dailyStrain.day))
      .limit(14)
    if (scoredStrain.length > 0 && scoredStrain[0]) {
      const latestStrainDay = scoredStrain[0].day
      const existingStrain = await db
        .select({ raw: dailyStrain.raw })
        .from(dailyStrain)
        .where(and(eq(dailyStrain.userId, userId), eq(dailyStrain.day, latestStrainDay)))
        .limit(1)
      const strainRaw = (existingStrain[0]?.raw as Record<string, unknown>) ?? {}
      strainRaw.daily = scoredStrain.map((r) => ({
        day: r.day,
        score_0_to_21: r.score,
      }))
      await db
        .update(dailyStrain)
        .set({ raw: strainRaw })
        .where(and(eq(dailyStrain.userId, userId), eq(dailyStrain.day, latestStrainDay)))
    }
  }

  // Packet-derived input reports (HRV, resting HR, steps, energy, motion, vital
  // events, daily/hourly rollups) — the map the app reads to render dashboards.
  // One latest row per user, computed over the whole store. The user's profile
  // (weight/age/sex/timezone) drives energy estimates + local-day bucketing.
  const inputReportsMap = await computeInputReports(core, dbPath, { profile })
  await db
    .insert(inputReports)
    .values({ userId, raw: inputReportsMap })
    .onConflictDoUpdate({
      target: inputReports.userId,
      set: { raw: inputReportsMap, computedAt: new Date() },
    })

  // Only prune after every durable projection above has succeeded. Prune is a
  // cache-maintenance step for the per-user SQLite workspace, not part of the
  // source-of-record ingest path; failure to prune should not turn a successful
  // projection into a compute failure.
  const cutoff = storeRetentionCutoffIso()
  try {
    await core.request("store.prune_raw_evidence_before", {
      database_path: dbPath,
      captured_before: cutoff,
    })
  } catch (error) {
    console.error(`[compute] ${userId} post-compute prune failed: ${errorMessage(error)}`)
  }
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
