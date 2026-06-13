import { beforeAll, describe, expect, test } from "bun:test"
import type { Env } from "../src/lib/env.ts"
import { ensureSchema, getDb } from "../src/db/client.ts"
import { upsertUserFromApple } from "../src/services/accounts.ts"
import { ingestMetrics } from "../src/services/metrics-ingest.ts"
import {
  dataSummary,
  listEnergy,
  listStrain,
  listStress,
  listVitals,
  restoreMetrics,
} from "../src/services/data-read.ts"

const TEST_DB = process.env.TEST_DATABASE_URL

// Exercises the real Postgres path for the curated metrics sync (Track A).
// Skipped when TEST_DATABASE_URL is absent so the unit suite stays green; CI
// (and the local run) provide a throwaway database.
const maybe = TEST_DB ? describe : describe.skip

maybe("curated metrics push + restore + idempotency (Postgres)", () => {
  const env = {
    DATABASE_URL: TEST_DB,
    APPLE_ISSUER: "https://appleid.apple.com",
    APPLE_BUNDLE_ID: "com.bull.swift",
  } as unknown as Env

  beforeAll(async () => {
    await ensureSchema(env)
  })

  test("push curated rows, restore them, re-push is idempotent", async () => {
    const db = getDb(env)
    expect(db).not.toBeNull()
    if (!db) return

    const sub = `apple-${crypto.randomUUID()}`
    const account = await upsertUserFromApple(db, { sub, isPrivateEmail: false }, "device-sync")
    const userId = account.userId

    const push = {
      source: "device_nightly_compute",
      recovery: [{ day: "2026-06-10", recovery_score: 71, hrv_ms: 64, resting_hr_bpm: 52 }],
      sleep: [{ day: "2026-06-10", sleep_score: 88, total_sleep_minutes: 451, rem_minutes: 96 }],
      strain: [{ day: "2026-06-10", strain_score: 12.4, kilojoules: 8200, avg_hr_bpm: 78, max_hr_bpm: 164 }],
      stress: [{ day: "2026-06-10", stress_score: 1.8, avg_stress: 1.5, max_stress: 2.9, high_stress_minutes: 42 }],
      energy: [{ day: "2026-06-10", energy_score: 64, energy_bank: 0.7, charge_rate: 12.5, drain_rate: 4.1 }],
      vitals: [
        {
          day: "2026-06-10",
          resting_hr_bpm: 52,
          hrv_ms: 64,
          respiratory_rate: 14.2,
          skin_temp_c: 33.1,
          spo2_pct: 96,
        },
      ],
      spo2: [{ recorded_at: "2026-06-10T03:00:00.000Z", spo2: 96 }],
    }

    const first = await ingestMetrics(db, userId, push)
    expect(first).toEqual({
      recovery: 1,
      sleep: 1,
      strain: 1,
      stress: 1,
      energy: 1,
      vitals: 1,
      spo2: 1,
    })

    // Restore returns the full curated history for the range, per family.
    const restored = await restoreMetrics(db, userId, { from: "2026-06-01", to: "2026-06-30", limit: 200 })
    expect(restored.recovery).toHaveLength(1)
    expect(restored.recovery[0]?.recoveryScore).toBe(71)
    expect(restored.strain[0]?.strainScore).toBe(12.4)
    expect(restored.strain[0]?.source).toBe("device_nightly_compute")
    expect(restored.stress[0]?.highStressMinutes).toBe(42)
    expect(restored.energy[0]?.energyBank).toBe(0.7)
    expect(restored.vitals[0]?.respiratoryRate).toBe(14.2)
    expect(restored.vitals[0]?.spo2Pct).toBe(96)

    // Summary counts every family.
    const summary = await dataSummary(db, userId)
    expect(summary.recovery_days).toBe(1)
    expect(summary.sleep_days).toBe(1)
    expect(summary.strain_days).toBe(1)
    expect(summary.stress_days).toBe(1)
    expect(summary.energy_days).toBe(1)
    expect(summary.vitals_days).toBe(1)
    expect(summary.spo2_samples).toBe(1)

    // Idempotency: re-pushing the same day upserts, never duplicates.
    await ingestMetrics(db, userId, push)
    const afterDup = await dataSummary(db, userId)
    expect(afterDup.strain_days).toBe(1)
    expect(afterDup.energy_days).toBe(1)
    const strainRows = await listStrain(db, userId, { limit: 200 })
    expect(strainRows).toHaveLength(1)

    // A corrected re-push updates the existing row in place.
    await ingestMetrics(db, userId, {
      source: "device_background_sync",
      strain: [{ day: "2026-06-10", strain_score: 15.9 }],
    })
    const correctedStrain = await listStrain(db, userId, { limit: 200 })
    expect(correctedStrain).toHaveLength(1)
    expect(correctedStrain[0]?.strainScore).toBe(15.9)
    expect(correctedStrain[0]?.source).toBe("device_background_sync")

    // Other families are untouched by a strain-only push.
    const stressRows = await listStress(db, userId, { limit: 200 })
    expect(stressRows[0]?.stressScore).toBe(1.8)
    const energyRows = await listEnergy(db, userId, { limit: 200 })
    expect(energyRows[0]?.energyScore).toBe(64)
    const vitalsRows = await listVitals(db, userId, { limit: 200 })
    expect(vitalsRows[0]?.skinTempC).toBe(33.1)
  })
})
