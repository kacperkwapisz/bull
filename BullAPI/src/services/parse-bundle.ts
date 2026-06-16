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
import { BullCore } from "../lib/bull-core.ts"
import type { ObjectStore } from "../lib/object-store.ts"

const DEVICE_MODEL = "WHOOP 5.0 Bull"
const LOCAL_BIOMETRIC_DEVICE_ID = "bull-local"

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

    // NOTE: writing the consolidated daily rows into the Postgres result tables
    // (dailyRecovery/dailySleep/etc.) is the next stage — it requires folding
    // the granular bull-core metric rows + scores into one row per day. The
    // compute above is what proves server-side parsing; results read-back lands
    // next. Mark the bundle parsed so the watermark advances.
    await db
      .update(uploadBundles)
      .set({ status: "parsed", parsedAt: new Date(), parseError: null })
      .where(eq(uploadBundles.id, bundleId))

    return {
      bundleId,
      frameCount: frames.length,
      status: "parsed",
      ...(pipeline.reports !== undefined ? { reports: pipeline.reports } : {}),
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
