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
import type { UserProfile } from "./profile.ts"

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

function pad2(n: number): string {
  return n < 10 ? `0${n}` : String(n)
}

/** Offset (ms) of `tz` at the given UTC instant: localWall − utcWall. */
function tzOffsetMs(utcMs: number, tz: string): number {
  const dtf = new Intl.DateTimeFormat("en-US", {
    timeZone: tz,
    hourCycle: "h23",
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  })
  const parts = dtf.formatToParts(new Date(utcMs))
  const get = (type: string): number => {
    const part = parts.find((p) => p.type === type)
    return part ? Number(part.value) : 0
  }
  const asUTC = Date.UTC(
    get("year"),
    get("month") - 1,
    get("day"),
    get("hour"),
    get("minute"),
    get("second"),
  )
  return asUTC - utcMs
}

/** The UTC instant for wall-clock (y, m, d, h) in `tz`. Handles day overflow
 * (d+1) and DST via the offset at the resulting instant. */
function zonedToUtc(y: number, m: number, d: number, h: number, tz: string): Date {
  const guess = Date.UTC(y, m - 1, d, h, 0, 0)
  return new Date(guess - tzOffsetMs(guess, tz))
}

/** Local calendar day window [start-of-day, start-of-next-day) for `tz`. */
function dailyWindow(now: Date, tz: string): MetricWindow {
  const local = new Date(now.getTime() + tzOffsetMs(now.getTime(), tz))
  const y = local.getUTCFullYear()
  const m = local.getUTCMonth() + 1
  const d = local.getUTCDate()
  const start = zonedToUtc(y, m, d, 0, tz)
  const end = zonedToUtc(y, m, d + 1, 0, tz)
  return {
    dateKey: `${y}-${pad2(m)}-${pad2(d)}`,
    timezone: tz,
    startISO: start.toISOString(),
    endISO: end.toISOString(),
    startMs: start.getTime(),
    endMs: end.getTime(),
  }
}

/** Local hour window [start-of-hour, +1h) for `tz`. */
function hourlyWindow(now: Date, tz: string): MetricWindow {
  const local = new Date(now.getTime() + tzOffsetMs(now.getTime(), tz))
  const y = local.getUTCFullYear()
  const m = local.getUTCMonth() + 1
  const d = local.getUTCDate()
  const h = local.getUTCHours()
  const start = zonedToUtc(y, m, d, h, tz)
  const end = new Date(start.getTime() + 3_600_000)
  return {
    dateKey: `${y}-${pad2(m)}-${pad2(d)}`,
    timezone: tz,
    startISO: start.toISOString(),
    endISO: end.toISOString(),
    startMs: start.getTime(),
    endMs: end.getTime(),
  }
}

function numberOrUndefined(value: unknown): number | undefined {
  return typeof value === "number" && Number.isFinite(value) ? value : undefined
}

/** Whole years from a `yyyy-MM-dd` birth date to now, or undefined if invalid /
 * outside the plausible 13..120 range. */
function ageYearsFrom(dateOfBirth: string | null | undefined): number | undefined {
  if (!dateOfBirth) return undefined
  const dob = new Date(`${dateOfBirth}T00:00:00Z`)
  if (Number.isNaN(dob.getTime())) return undefined
  const now = new Date()
  let years = now.getUTCFullYear() - dob.getUTCFullYear()
  const monthDelta = now.getUTCMonth() - dob.getUTCMonth()
  if (monthDelta < 0 || (monthDelta === 0 && now.getUTCDate() < dob.getUTCDate())) years -= 1
  return years >= 13 && years <= 120 ? years : undefined
}

/** Energy-model args derived from the user's profile (weight/age/sex). Each is
 * included only when valid, so calorie output degrades honestly when a field is
 * missing instead of being guessed. */
function profileEnergyArgs(profile: UserProfile | null | undefined): Record<string, number | string> {
  const args: Record<string, number | string> = {}
  if (!profile) return args
  if (profile.weightGrams != null) {
    const weightKg = profile.weightGrams / 1000
    if (weightKg >= 25 && weightKg <= 300) args.profile_weight_kg = weightKg
  }
  const age = ageYearsFrom(profile.dateOfBirth)
  if (age !== undefined) {
    args.profile_age_years = age
    args.max_hr_bpm = Math.max(120, Math.min(210, 208 - 0.7 * age))
  }
  if (profile.sex === "male" || profile.sex === "female") args.profile_sex = profile.sex
  return args
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
  options: { profile?: UserProfile | null; now?: Date } = {},
): Promise<Record<string, unknown>> {
  const now = options.now ?? new Date()
  const tz = options.profile?.timezone || "UTC"
  const daily = dailyWindow(now, tz)
  const hourly = hourlyWindow(now, tz)
  const energyProfileArgs = profileEnergyArgs(options.profile)
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
  // same base args plus the profile-derived energy fields (weight/age/sex). Any
  // profile field that's missing is simply omitted, so calorie output degrades
  // honestly instead of guessing.
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
    ...energyProfileArgs,
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
    ...energyProfileArgs,
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
