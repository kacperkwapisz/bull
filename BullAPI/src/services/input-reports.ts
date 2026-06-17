/**
 * Server-side packet-derived input reports.
 *
 * Runs the same bull-core metric methods the device used to run on-phone, over a
 * user's compute store, and assembles the report map the app reads to render its
 * dashboards (HRV, resting HR, steps, energy, motion, vital events, daily/hourly
 * rollups, and honest-unavailable statuses). Producing this server-side lets the
 * phone become a thin relay: every value still originates from the connected
 * device's own sensor frames, just computed once on the server and read back.
 *
 * Orchestration only — the actual feature math lives in bull-core (shared by the
 * sidecar). Each call is isolated so one flaky method degrades a single key
 * instead of failing the whole map.
 */

import type { BullCore } from "../lib/bull-core.ts"

// Keys the connected device's surfaced biometric streams are stored under in the
// per-user compute store (gravity, SpO2, skin temp, resp). Must match the id the
// app uses so ingest and read-back agree.
const BIOMETRIC_DEVICE_ID = "bull.device.local.v1"

interface MetricWindow {
  dateKey: string
  timezone: string
  startISO: string
  endISO: string
  startMs: number
  endMs: number
}

/**
 * UTC day window [start-of-day, +1d). The device computed these in its local
 * timezone; server-side we use UTC until the app uploads its timezone, so the
 * daily rollups bucket on the UTC calendar day.
 */
function dailyWindow(now: Date): MetricWindow {
  const start = new Date(Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate()))
  const end = new Date(start.getTime() + 86_400_000)
  return {
    dateKey: start.toISOString().slice(0, 10),
    timezone: "UTC",
    startISO: start.toISOString(),
    endISO: end.toISOString(),
    startMs: start.getTime(),
    endMs: end.getTime(),
  }
}

/** UTC hour window [start-of-hour, +1h). */
function hourlyWindow(now: Date): MetricWindow {
  const start = new Date(
    Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate(), now.getUTCHours()),
  )
  const end = new Date(start.getTime() + 3_600_000)
  return {
    dateKey: start.toISOString().slice(0, 10),
    timezone: "UTC",
    startISO: start.toISOString(),
    endISO: end.toISOString(),
    startMs: start.getTime(),
    endMs: end.getTime(),
  }
}

function numberOrUndefined(value: unknown): number | undefined {
  return typeof value === "number" && Number.isFinite(value) ? value : undefined
}

/**
 * Run the full packet-derived input pipeline over `dbPath` and return the report
 * map keyed exactly as the app expects (motion, hrv, resting_hr, energy_rollup,
 * daily_recovery, …). Method order matches the device's: stream ingest before
 * the feature reads, the resting-HR rollup before energy (which consumes it).
 */
export async function computeInputReports(
  core: BullCore,
  dbPath: string,
  now: Date = new Date(),
): Promise<Record<string, unknown>> {
  const daily = dailyWindow(now)
  const hourly = hourlyWindow(now)
  const base = {
    database_path: dbPath,
    start: "0000",
    end: "9999",
    min_owned_captures: 2,
    require_trusted_evidence: false,
  }
  const reports: Record<string, unknown> = {}
  const call = async (key: string, method: string, args: object): Promise<void> => {
    try {
      reports[key] = await core.request(method, args)
    } catch (error) {
      reports[key] = { error: error instanceof Error ? error.message : String(error) }
    }
  }

  await call("readiness", "metrics.input_readiness", {
    database_path: dbPath,
    start: "0000",
    end: "9999",
    min_owned_captures: 2,
    require_owned_captures: false,
    require_scores_ready: true,
  })
  await call("motion", "metrics.motion_features", base)
  await call("step_discovery", "metrics.step_packet_discovery", {
    ...base,
    max_candidate_fields: 100,
  })
  await call("step_counter_ingest", "metrics.step_counter_ingest", {
    ...base,
    max_candidate_fields: 1_000,
  })
  await call("biometric_ingest", "biometrics.ingest_from_decoded", {
    database_path: dbPath,
    device_id: BIOMETRIC_DEVICE_ID,
    start: "0000",
    end: "9999",
  })
  await call("heart_rate", "metrics.heart_rate_features", base)
  await call("vital_event", "metrics.vital_event_features", base)
  await call("hrv", "metrics.hrv_features", {
    ...base,
    min_rr_intervals_to_compute: 2,
    baseline_min_days: 3,
    require_baseline: false,
  })
  await call("window", "metrics.window_features", base)
  await call("resting_hr", "metrics.resting_hr_features", {
    ...base,
    baseline_min_days: 3,
    require_baseline: false,
  })

  await call("resting_hr_rollup", "metrics.resting_hr_daily_rollup", {
    database_path: dbPath,
    date_key: daily.dateKey,
    timezone: daily.timezone,
    start: daily.startISO,
    end: daily.endISO,
    min_owned_captures: 2,
    require_trusted_evidence: false,
    baseline_min_days: 3,
    require_baseline: false,
    min_sample_count: 2,
    write_metric: true,
  })
  const restingHrBpm = numberOrUndefined(
    (reports.resting_hr_rollup as Record<string, unknown> | undefined)?.resting_hr_bpm,
  )

  await call("step_counter_rollup", "metrics.step_counter_daily_rollup", {
    database_path: dbPath,
    date_key: daily.dateKey,
    timezone: daily.timezone,
    start_time_unix_ms: daily.startMs,
    end_time_unix_ms: daily.endMs,
    min_sample_count: 2,
    write_metric: true,
  })
  await call("step_counter_hourly_rollup", "metrics.step_counter_hourly_rollup", {
    database_path: dbPath,
    date_key: hourly.dateKey,
    timezone: hourly.timezone,
    start_time_unix_ms: hourly.startMs,
    end_time_unix_ms: hourly.endMs,
    min_sample_count: 2,
    write_metric: true,
  })
  await call("activity_unavailable_status", "metrics.activity_unavailable_daily_status", {
    database_path: dbPath,
    date_key: daily.dateKey,
    timezone: daily.timezone,
    start_time_unix_ms: daily.startMs,
    end_time_unix_ms: daily.endMs,
    min_sample_count: 2,
    write_metric: true,
  })

  // Energy needs the resting-HR rollup as an input; daily and hourly share the
  // same base args (profile fields — weight/age/sex — are omitted until the app
  // uploads its profile, so calorie output is degraded but everything else is
  // computed).
  const energyDailyArgs = {
    database_path: dbPath,
    date_key: daily.dateKey,
    timezone: daily.timezone,
    start: daily.startISO,
    end: daily.endISO,
    min_owned_captures: 2,
    require_trusted_evidence: false,
    min_heart_rate_samples: 2,
    write_metric: true,
    ...(restingHrBpm !== undefined ? { resting_hr_bpm: restingHrBpm } : {}),
  }
  await call("energy_rollup", "metrics.energy_daily_rollup", energyDailyArgs)
  await call("energy_hourly_rollup", "metrics.energy_hourly_rollup", {
    database_path: dbPath,
    date_key: hourly.dateKey,
    timezone: hourly.timezone,
    start: hourly.startISO,
    end: hourly.endISO,
    min_owned_captures: 2,
    require_trusted_evidence: false,
    min_heart_rate_samples: 2,
    write_metric: true,
    ...(restingHrBpm !== undefined ? { resting_hr_bpm: restingHrBpm } : {}),
  })
  await call("energy_unavailable_status", "metrics.energy_unavailable_daily_status", energyDailyArgs)

  const recoveryStatusArgs = {
    database_path: dbPath,
    date_key: daily.dateKey,
    timezone: daily.timezone,
    start: daily.startISO,
    end: daily.endISO,
    min_owned_captures: 2,
    require_trusted_evidence: false,
    min_rr_intervals_to_compute: 2,
    write_metric: true,
  }
  await call("recovery_sensor_rollup", "metrics.recovery_sensor_daily_rollup", recoveryStatusArgs)
  await call(
    "recovery_unavailable_status",
    "metrics.recovery_unavailable_daily_status",
    recoveryStatusArgs,
  )

  // List reports carry trailing history for trends: 30 days of daily rows, 48h
  // of hourly rows.
  const dailyHistoryStartMs = daily.startMs - 29 * 86_400_000
  await call("daily_activity", "metrics.daily_activity_metrics", {
    database_path: dbPath,
    start_time_unix_ms: dailyHistoryStartMs,
    end_time_unix_ms: daily.endMs,
  })
  await call("hourly_activity", "metrics.hourly_activity_metrics", {
    database_path: dbPath,
    start_time_unix_ms: hourly.startMs - 48 * 3_600_000,
    end_time_unix_ms: hourly.endMs,
  })
  await call("daily_recovery", "metrics.daily_recovery_metrics", {
    database_path: dbPath,
    start_time_unix_ms: dailyHistoryStartMs,
    end_time_unix_ms: daily.endMs,
  })

  return reports
}
