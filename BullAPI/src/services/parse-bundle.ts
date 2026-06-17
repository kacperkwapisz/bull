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
import { eq } from "drizzle-orm"
import type { Db } from "../db/client.ts"
import { uploadBundles } from "../db/schema.ts"
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
      device_type: "Bull" as const,
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
    await core.request("metrics.sleep_score_from_features", {
      database_path: dbPath,
      ...SLEEP_SCORE_ARGS,
    })

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
