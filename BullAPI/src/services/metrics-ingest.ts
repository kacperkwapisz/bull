/**
 * Curated daily-metrics ingest.
 *
 * The device computes its metrics locally from the connected device's own live
 * sensor data, then pushes a curated daily rollup here so the server can act as
 * the long-term clean-data store (restore-on-reinstall, web reads). This path
 * is independent of the raw-archive object store: it writes only into the
 * curated projection tables.
 *
 * Idempotency: every row is keyed by (user, day) within its metric family and
 * upserted, so re-pushing the same day — after a correction or a duplicate
 * background run — converges to the same state instead of duplicating rows.
 * Each value originates from the device's own sensors; nothing here is imported
 * from third-party health stores.
 */

import { z } from "zod"
import { sql } from "drizzle-orm"
import type { Db } from "../db/client.ts"
import {
  dailyEnergy,
  dailyRecovery,
  dailySleep,
  dailyStrain,
  dailyStress,
  spo2Samples,
  vitalsDaily,
} from "../db/schema.ts"

const dayString = z.string().regex(/^\d{4}-\d{2}-\d{2}$/)
const rawObject = z.record(z.string(), z.unknown()).optional()

const recoveryRow = z.object({
  day: dayString,
  recovery_score: z.number().nullable().optional(),
  hrv_ms: z.number().nullable().optional(),
  resting_hr_bpm: z.number().nullable().optional(),
  raw: rawObject,
})

const sleepRow = z.object({
  day: dayString,
  sleep_score: z.number().nullable().optional(),
  total_sleep_minutes: z.number().nullable().optional(),
  rem_minutes: z.number().nullable().optional(),
  deep_minutes: z.number().nullable().optional(),
  light_minutes: z.number().nullable().optional(),
  awake_minutes: z.number().nullable().optional(),
  raw: rawObject,
})

const strainRow = z.object({
  day: dayString,
  strain_score: z.number().nullable().optional(),
  kilojoules: z.number().nullable().optional(),
  avg_hr_bpm: z.number().nullable().optional(),
  max_hr_bpm: z.number().nullable().optional(),
  raw: rawObject,
})

const stressRow = z.object({
  day: dayString,
  stress_score: z.number().nullable().optional(),
  avg_stress: z.number().nullable().optional(),
  max_stress: z.number().nullable().optional(),
  high_stress_minutes: z.number().nullable().optional(),
  raw: rawObject,
})

const energyRow = z.object({
  day: dayString,
  energy_score: z.number().nullable().optional(),
  energy_bank: z.number().nullable().optional(),
  charge_rate: z.number().nullable().optional(),
  drain_rate: z.number().nullable().optional(),
  raw: rawObject,
})

const vitalsRow = z.object({
  day: dayString,
  resting_hr_bpm: z.number().nullable().optional(),
  hrv_ms: z.number().nullable().optional(),
  respiratory_rate: z.number().nullable().optional(),
  skin_temp_c: z.number().nullable().optional(),
  spo2_pct: z.number().nullable().optional(),
  raw: rawObject,
})

const spo2SampleRow = z.object({
  recorded_at: z.string().datetime(),
  spo2: z.number().nullable().optional(),
  raw: rawObject,
})

export const metricsPushSchema = z.object({
  // Free-text provenance applied to every curated row in this push, e.g.
  // "device_nightly_compute" or "device_background_sync".
  source: z.string().min(1).max(120).optional(),
  recovery: z.array(recoveryRow).optional(),
  sleep: z.array(sleepRow).optional(),
  strain: z.array(strainRow).optional(),
  stress: z.array(stressRow).optional(),
  energy: z.array(energyRow).optional(),
  vitals: z.array(vitalsRow).optional(),
  spo2: z.array(spo2SampleRow).optional(),
})

export type MetricsPush = z.infer<typeof metricsPushSchema>

export interface MetricsIngestResult {
  readonly recovery: number
  readonly sleep: number
  readonly strain: number
  readonly stress: number
  readonly energy: number
  readonly vitals: number
  readonly spo2: number
}

export async function ingestMetrics(
  db: Db,
  userId: string,
  push: MetricsPush,
): Promise<MetricsIngestResult> {
  const source = push.source ?? null

  for (const r of push.recovery ?? []) {
    await db
      .insert(dailyRecovery)
      .values({
        userId,
        day: r.day,
        recoveryScore: r.recovery_score ?? null,
        hrvMs: r.hrv_ms ?? null,
        restingHrBpm: r.resting_hr_bpm ?? null,
        raw: r.raw ?? null,
      })
      .onConflictDoUpdate({
        target: [dailyRecovery.userId, dailyRecovery.day],
        set: {
          recoveryScore: r.recovery_score ?? null,
          hrvMs: r.hrv_ms ?? null,
          restingHrBpm: r.resting_hr_bpm ?? null,
          raw: r.raw ?? null,
        },
      })
  }

  for (const s of push.sleep ?? []) {
    await db
      .insert(dailySleep)
      .values({
        userId,
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

  for (const r of push.strain ?? []) {
    await db
      .insert(dailyStrain)
      .values({
        userId,
        day: r.day,
        strainScore: r.strain_score ?? null,
        kilojoules: r.kilojoules ?? null,
        avgHrBpm: r.avg_hr_bpm ?? null,
        maxHrBpm: r.max_hr_bpm ?? null,
        source,
        raw: r.raw ?? null,
      })
      .onConflictDoUpdate({
        target: [dailyStrain.userId, dailyStrain.day],
        set: {
          strainScore: r.strain_score ?? null,
          kilojoules: r.kilojoules ?? null,
          avgHrBpm: r.avg_hr_bpm ?? null,
          maxHrBpm: r.max_hr_bpm ?? null,
          source,
          raw: r.raw ?? null,
          updatedAt: sql`now()`,
        },
      })
  }

  for (const r of push.stress ?? []) {
    await db
      .insert(dailyStress)
      .values({
        userId,
        day: r.day,
        stressScore: r.stress_score ?? null,
        avgStress: r.avg_stress ?? null,
        maxStress: r.max_stress ?? null,
        highStressMinutes: r.high_stress_minutes ?? null,
        source,
        raw: r.raw ?? null,
      })
      .onConflictDoUpdate({
        target: [dailyStress.userId, dailyStress.day],
        set: {
          stressScore: r.stress_score ?? null,
          avgStress: r.avg_stress ?? null,
          maxStress: r.max_stress ?? null,
          highStressMinutes: r.high_stress_minutes ?? null,
          source,
          raw: r.raw ?? null,
          updatedAt: sql`now()`,
        },
      })
  }

  for (const r of push.energy ?? []) {
    await db
      .insert(dailyEnergy)
      .values({
        userId,
        day: r.day,
        energyScore: r.energy_score ?? null,
        energyBank: r.energy_bank ?? null,
        chargeRate: r.charge_rate ?? null,
        drainRate: r.drain_rate ?? null,
        source,
        raw: r.raw ?? null,
      })
      .onConflictDoUpdate({
        target: [dailyEnergy.userId, dailyEnergy.day],
        set: {
          energyScore: r.energy_score ?? null,
          energyBank: r.energy_bank ?? null,
          chargeRate: r.charge_rate ?? null,
          drainRate: r.drain_rate ?? null,
          source,
          raw: r.raw ?? null,
          updatedAt: sql`now()`,
        },
      })
  }

  for (const r of push.vitals ?? []) {
    await db
      .insert(vitalsDaily)
      .values({
        userId,
        day: r.day,
        restingHrBpm: r.resting_hr_bpm ?? null,
        hrvMs: r.hrv_ms ?? null,
        respiratoryRate: r.respiratory_rate ?? null,
        skinTempC: r.skin_temp_c ?? null,
        spo2Pct: r.spo2_pct ?? null,
        source,
        raw: r.raw ?? null,
      })
      .onConflictDoUpdate({
        target: [vitalsDaily.userId, vitalsDaily.day],
        set: {
          restingHrBpm: r.resting_hr_bpm ?? null,
          hrvMs: r.hrv_ms ?? null,
          respiratoryRate: r.respiratory_rate ?? null,
          skinTempC: r.skin_temp_c ?? null,
          spo2Pct: r.spo2_pct ?? null,
          source,
          raw: r.raw ?? null,
          updatedAt: sql`now()`,
        },
      })
  }

  for (const p of push.spo2 ?? []) {
    await db
      .insert(spo2Samples)
      .values({
        userId,
        recordedAt: new Date(p.recorded_at),
        spo2: p.spo2 ?? null,
        raw: p.raw ?? null,
      })
      .onConflictDoUpdate({
        target: [spo2Samples.userId, spo2Samples.recordedAt],
        set: {
          spo2: p.spo2 ?? null,
          raw: p.raw ?? null,
        },
      })
  }

  return {
    recovery: push.recovery?.length ?? 0,
    sleep: push.sleep?.length ?? 0,
    strain: push.strain?.length ?? 0,
    stress: push.stress?.length ?? 0,
    energy: push.energy?.length ?? 0,
    vitals: push.vitals?.length ?? 0,
    spo2: push.spo2?.length ?? 0,
  }
}
