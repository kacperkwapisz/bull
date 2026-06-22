/**
 * Read queries for the web app + debugging. All scoped to a single user.
 * Honest empty states: missing data returns empty arrays / nulls, never guesses.
 */

import { and, desc, eq, gte, lte, sql } from "drizzle-orm"
import type { Db } from "../db/client.ts"
import {
  dailyEnergy,
  dailyRecovery,
  dailySleep,
  dailyStrain,
  dailyStress,
  inputReports,
  spo2Samples,
  uploadBundles,
  vitalsDaily,
} from "../db/schema.ts"

export interface BundleRef {
  readonly id: string
  readonly storageKey: string
  readonly contentType: string
  readonly byteSize: number
  readonly checksum: string
}

/** Look up a single bundle owned by the user, for presigned download. */
export async function getBundleForUser(
  db: Db,
  userId: string,
  bundleId: string,
): Promise<BundleRef | null> {
  const rows = await db
    .select({
      id: uploadBundles.id,
      storageKey: uploadBundles.storageKey,
      contentType: uploadBundles.contentType,
      byteSize: uploadBundles.byteSize,
      checksum: uploadBundles.checksum,
    })
    .from(uploadBundles)
    .where(and(eq(uploadBundles.id, bundleId), eq(uploadBundles.userId, userId)))
    .limit(1)
  return rows[0] ?? null
}

export interface DayRange {
  readonly from?: string
  readonly to?: string
  readonly limit: number
}

export async function listRecovery(db: Db, userId: string, range: DayRange) {
  const conds = [eq(dailyRecovery.userId, userId)]
  if (range.from) conds.push(gte(dailyRecovery.day, range.from))
  if (range.to) conds.push(lte(dailyRecovery.day, range.to))
  return db
    .select()
    .from(dailyRecovery)
    .where(and(...conds))
    .orderBy(desc(dailyRecovery.day))
    .limit(range.limit)
}

export async function listSleep(db: Db, userId: string, range: DayRange) {
  const conds = [eq(dailySleep.userId, userId)]
  if (range.from) conds.push(gte(dailySleep.day, range.from))
  if (range.to) conds.push(lte(dailySleep.day, range.to))
  return db
    .select()
    .from(dailySleep)
    .where(and(...conds))
    .orderBy(desc(dailySleep.day))
    .limit(range.limit)
}

export async function listStrain(db: Db, userId: string, range: DayRange) {
  const conds = [eq(dailyStrain.userId, userId)]
  if (range.from) conds.push(gte(dailyStrain.day, range.from))
  if (range.to) conds.push(lte(dailyStrain.day, range.to))
  return db
    .select()
    .from(dailyStrain)
    .where(and(...conds))
    .orderBy(desc(dailyStrain.day))
    .limit(range.limit)
}

export async function listStress(db: Db, userId: string, range: DayRange) {
  const conds = [eq(dailyStress.userId, userId)]
  if (range.from) conds.push(gte(dailyStress.day, range.from))
  if (range.to) conds.push(lte(dailyStress.day, range.to))
  return db
    .select()
    .from(dailyStress)
    .where(and(...conds))
    .orderBy(desc(dailyStress.day))
    .limit(range.limit)
}

export async function listEnergy(db: Db, userId: string, range: DayRange) {
  const conds = [eq(dailyEnergy.userId, userId)]
  if (range.from) conds.push(gte(dailyEnergy.day, range.from))
  if (range.to) conds.push(lte(dailyEnergy.day, range.to))
  return db
    .select()
    .from(dailyEnergy)
    .where(and(...conds))
    .orderBy(desc(dailyEnergy.day))
    .limit(range.limit)
}

export async function listVitals(db: Db, userId: string, range: DayRange) {
  const conds = [eq(vitalsDaily.userId, userId)]
  if (range.from) conds.push(gte(vitalsDaily.day, range.from))
  if (range.to) conds.push(lte(vitalsDaily.day, range.to))
  return db
    .select()
    .from(vitalsDaily)
    .where(and(...conds))
    .orderBy(desc(vitalsDaily.day))
    .limit(range.limit)
}

export async function listSpo2(db: Db, userId: string, limit: number) {
  return db
    .select()
    .from(spo2Samples)
    .where(eq(spo2Samples.userId, userId))
    .orderBy(desc(spo2Samples.recordedAt))
    .limit(limit)
}

export async function listUploads(db: Db, userId: string, limit: number) {
  return db
    .select({
      id: uploadBundles.id,
      deviceId: uploadBundles.deviceId,
      checksum: uploadBundles.checksum,
      byteSize: uploadBundles.byteSize,
      status: uploadBundles.status,
      timeframeStart: uploadBundles.timeframeStart,
      timeframeEnd: uploadBundles.timeframeEnd,
      parseError: uploadBundles.parseError,
      createdAt: uploadBundles.createdAt,
      parsedAt: uploadBundles.parsedAt,
    })
    .from(uploadBundles)
    .where(eq(uploadBundles.userId, userId))
    .orderBy(desc(uploadBundles.createdAt))
    .limit(limit)
}

export interface DataSummary {
  readonly recovery_days: number
  readonly sleep_days: number
  readonly strain_days: number
  readonly stress_days: number
  readonly energy_days: number
  readonly vitals_days: number
  readonly spo2_samples: number
  readonly uploads: number
  readonly latest_recovery_day: string | null
  readonly latest_sleep_day: string | null
}

export async function dataSummary(db: Db, userId: string): Promise<DataSummary> {
  const [rec, slp, str, sts, ene, vit, ox, up] = await Promise.all([
    db
      .select({ n: sql<number>`count(*)`, latest: sql<string | null>`max(${dailyRecovery.day})` })
      .from(dailyRecovery)
      .where(eq(dailyRecovery.userId, userId)),
    db
      .select({ n: sql<number>`count(*)`, latest: sql<string | null>`max(${dailySleep.day})` })
      .from(dailySleep)
      .where(eq(dailySleep.userId, userId)),
    db
      .select({ n: sql<number>`count(*)` })
      .from(dailyStrain)
      .where(eq(dailyStrain.userId, userId)),
    db
      .select({ n: sql<number>`count(*)` })
      .from(dailyStress)
      .where(eq(dailyStress.userId, userId)),
    db
      .select({ n: sql<number>`count(*)` })
      .from(dailyEnergy)
      .where(eq(dailyEnergy.userId, userId)),
    db
      .select({ n: sql<number>`count(*)` })
      .from(vitalsDaily)
      .where(eq(vitalsDaily.userId, userId)),
    db
      .select({ n: sql<number>`count(*)` })
      .from(spo2Samples)
      .where(eq(spo2Samples.userId, userId)),
    db
      .select({ n: sql<number>`count(*)` })
      .from(uploadBundles)
      .where(eq(uploadBundles.userId, userId)),
  ])
  return {
    recovery_days: Number(rec[0]?.n ?? 0),
    sleep_days: Number(slp[0]?.n ?? 0),
    strain_days: Number(str[0]?.n ?? 0),
    stress_days: Number(sts[0]?.n ?? 0),
    energy_days: Number(ene[0]?.n ?? 0),
    vitals_days: Number(vit[0]?.n ?? 0),
    spo2_samples: Number(ox[0]?.n ?? 0),
    uploads: Number(up[0]?.n ?? 0),
    latest_recovery_day: rec[0]?.latest ?? null,
    latest_sleep_day: slp[0]?.latest ?? null,
  }
}

/** Latest packet-derived input-report map for a user (the dashboard read), or
 * null if nothing has been computed yet (honest-empty). */
export async function getInputReports(db: Db, userId: string) {
  const rows = await db
    .select({ raw: inputReports.raw, computedAt: inputReports.computedAt })
    .from(inputReports)
    .where(eq(inputReports.userId, userId))
    .limit(1)
  return rows[0] ?? null
}

// ---------------------------------------------------------------------------
// BFF: single home payload — everything the app needs in one round-trip.
// ---------------------------------------------------------------------------

export interface HomePayload {
  readonly recovery: Record<string, unknown> | null
  readonly sleep: Record<string, unknown> | null
  readonly strain: Record<string, unknown> | null
  readonly stress: Record<string, unknown> | null
  readonly energy: Record<string, unknown> | null
  readonly vitals: Record<string, unknown> | null
  readonly inputs: Record<string, unknown> | null
  readonly computed_at: string | null
}

/** Fetch the latest row from each score/input table in parallel and return a
 *  single object the client can paint its entire home + health surface from. */
export async function fetchHome(db: Db, userId: string): Promise<HomePayload> {
  const latestRaw = <T extends { raw?: unknown }>(rows: T[]): Record<string, unknown> | null =>
    (rows[0]?.raw as Record<string, unknown>) ?? null

  const [rec, slp, str, sts, ene, vit, inp] = await Promise.all([
    db.select({ raw: dailyRecovery.raw }).from(dailyRecovery)
      .where(eq(dailyRecovery.userId, userId)).orderBy(desc(dailyRecovery.day)).limit(1),
    db.select({ raw: dailySleep.raw }).from(dailySleep)
      .where(eq(dailySleep.userId, userId)).orderBy(desc(dailySleep.day)).limit(1),
    db.select({ raw: dailyStrain.raw }).from(dailyStrain)
      .where(eq(dailyStrain.userId, userId)).orderBy(desc(dailyStrain.day)).limit(1),
    db.select({ raw: dailyStress.raw }).from(dailyStress)
      .where(eq(dailyStress.userId, userId)).orderBy(desc(dailyStress.day)).limit(1),
    db.select({ raw: dailyEnergy.raw }).from(dailyEnergy)
      .where(eq(dailyEnergy.userId, userId)).orderBy(desc(dailyEnergy.day)).limit(1),
    db.select({ raw: vitalsDaily.raw }).from(vitalsDaily)
      .where(eq(vitalsDaily.userId, userId)).orderBy(desc(vitalsDaily.day)).limit(1),
    db.select({ raw: inputReports.raw, computedAt: inputReports.computedAt }).from(inputReports)
      .where(eq(inputReports.userId, userId)).limit(1),
  ])

  return {
    recovery: latestRaw(rec),
    sleep: latestRaw(slp),
    strain: latestRaw(str),
    stress: latestRaw(sts),
    energy: latestRaw(ene),
    vitals: latestRaw(vit),
    inputs: (inp[0]?.raw as Record<string, unknown>) ?? null,
    computed_at: inp[0]?.computedAt?.toISOString() ?? null,
  }
}
