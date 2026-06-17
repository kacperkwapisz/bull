/**
 * BullAPI persistence schema (Postgres / Drizzle).
 *
 * Scope: store device-originated WHOOP 5 data per Bull user so the web app can
 * read it and so raw uploads can be inspected for debugging. Every physiological
 * row here originates from the connected device's own live sensor data, uploaded
 * by the Bull app; BullAPI never ingests physiology from third-party health
 * stores. The raw upload bundle is kept as the source of record; curated tables
 * below are a queryable projection of it that can be re-derived at any time.
 */

import {
  bigint,
  date,
  doublePrecision,
  index,
  integer,
  jsonb,
  pgTable,
  text,
  timestamp,
  uniqueIndex,
  uuid,
} from "drizzle-orm/pg-core"

export const users = pgTable("users", {
  id: uuid("id").primaryKey().defaultRandom(),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  lastSeenAt: timestamp("last_seen_at", { withTimezone: true }).notNull().defaultNow(),
})

export const appleIdentities = pgTable(
  "apple_identities",
  {
    id: uuid("id").primaryKey().defaultRandom(),
    userId: uuid("user_id")
      .notNull()
      .references(() => users.id, { onDelete: "cascade" }),
    // Apple's stable subject ("sub") claim — the per-app user identifier.
    appleSub: text("apple_sub").notNull(),
    email: text("email"),
    isPrivateEmail: integer("is_private_email").notNull().default(0),
    createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  },
  (t) => ({
    appleSubUnique: uniqueIndex("apple_identities_apple_sub_uq").on(t.appleSub),
  }),
)

export const devices = pgTable(
  "devices",
  {
    id: uuid("id").primaryKey().defaultRandom(),
    userId: uuid("user_id")
      .notNull()
      .references(() => users.id, { onDelete: "cascade" }),
    // Client-supplied stable device identifier (e.g. identifierForVendor).
    deviceId: text("device_id").notNull(),
    platform: text("platform").notNull().default("ios"),
    createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
    lastSeenAt: timestamp("last_seen_at", { withTimezone: true }).notNull().defaultNow(),
  },
  (t) => ({
    userDeviceUnique: uniqueIndex("devices_user_device_uq").on(t.userId, t.deviceId),
  }),
)

/**
 * One row per uploaded export bundle. The raw bytes live in object storage
 * (S3/R2) under storageKey; checksum makes re-uploads idempotent per user.
 * status tracks the parse lifecycle so debugging can see exactly what arrived.
 */
export const uploadBundles = pgTable(
  "upload_bundles",
  {
    id: uuid("id").primaryKey().defaultRandom(),
    userId: uuid("user_id")
      .notNull()
      .references(() => users.id, { onDelete: "cascade" }),
    deviceId: text("device_id"),
    // sha256 of the raw bundle bytes.
    checksum: text("checksum").notNull(),
    byteSize: bigint("byte_size", { mode: "number" }).notNull(),
    // pending | parsed | failed
    status: text("status").notNull().default("pending"),
    // Object-storage key for the raw bundle bytes (S3/R2).
    storageKey: text("storage_key").notNull(),
    contentType: text("content_type").notNull().default("application/octet-stream"),
    timeframeStart: timestamp("timeframe_start", { withTimezone: true }),
    timeframeEnd: timestamp("timeframe_end", { withTimezone: true }),
    parseError: text("parse_error"),
    createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
    parsedAt: timestamp("parsed_at", { withTimezone: true }),
  },
  (t) => ({
    userChecksumUnique: uniqueIndex("upload_bundles_user_checksum_uq").on(t.userId, t.checksum),
    userCreatedIdx: index("upload_bundles_user_created_idx").on(t.userId, t.createdAt),
  }),
)

export const dailyRecovery = pgTable(
  "daily_recovery",
  {
    id: uuid("id").primaryKey().defaultRandom(),
    userId: uuid("user_id")
      .notNull()
      .references(() => users.id, { onDelete: "cascade" }),
    sourceBundleId: uuid("source_bundle_id").references(() => uploadBundles.id, {
      onDelete: "set null",
    }),
    day: date("day").notNull(),
    recoveryScore: doublePrecision("recovery_score"),
    hrvMs: doublePrecision("hrv_ms"),
    restingHrBpm: doublePrecision("resting_hr_bpm"),
    raw: jsonb("raw"),
    createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  },
  (t) => ({
    userDayUnique: uniqueIndex("daily_recovery_user_day_uq").on(t.userId, t.day),
  }),
)

// Per-user profile + timezone the app uploads so server-side compute can derive
// energy/calorie estimates (weight, age, sex) and bucket daily rollups on the
// user's local calendar day. Optional fields: compute degrades gracefully when
// absent (honest output, no guessed values).
export const userProfiles = pgTable(
  "user_profiles",
  {
    id: uuid("id").primaryKey().defaultRandom(),
    userId: uuid("user_id")
      .notNull()
      .references(() => users.id, { onDelete: "cascade" }),
    weightGrams: integer("weight_grams"),
    dateOfBirth: date("date_of_birth"),
    sex: text("sex"),
    timezone: text("timezone"),
    updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow(),
  },
  (t) => ({
    userUnique: uniqueIndex("user_profiles_user_uq").on(t.userId),
  }),
)

// The full packet-derived input-report map (motion, HRV, resting HR, steps,
// energy, vital events, daily/hourly rollups, honest-unavailable statuses)
// computed server-side over the user's store. The app reads this verbatim to
// populate its dashboards — one latest row per user, re-derivable from the raw
// bundles at any time.
export const inputReports = pgTable(
  "input_reports",
  {
    id: uuid("id").primaryKey().defaultRandom(),
    userId: uuid("user_id")
      .notNull()
      .references(() => users.id, { onDelete: "cascade" }),
    raw: jsonb("raw").notNull(),
    computedAt: timestamp("computed_at", { withTimezone: true }).notNull().defaultNow(),
  },
  (t) => ({
    userUnique: uniqueIndex("input_reports_user_uq").on(t.userId),
  }),
)

export const dailySleep = pgTable(
  "daily_sleep",
  {
    id: uuid("id").primaryKey().defaultRandom(),
    userId: uuid("user_id")
      .notNull()
      .references(() => users.id, { onDelete: "cascade" }),
    sourceBundleId: uuid("source_bundle_id").references(() => uploadBundles.id, {
      onDelete: "set null",
    }),
    day: date("day").notNull(),
    sleepScore: doublePrecision("sleep_score"),
    totalSleepMinutes: doublePrecision("total_sleep_minutes"),
    remMinutes: doublePrecision("rem_minutes"),
    deepMinutes: doublePrecision("deep_minutes"),
    lightMinutes: doublePrecision("light_minutes"),
    awakeMinutes: doublePrecision("awake_minutes"),
    raw: jsonb("raw"),
    createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  },
  (t) => ({
    userDayUnique: uniqueIndex("daily_sleep_user_day_uq").on(t.userId, t.day),
  }),
)

export const spo2Samples = pgTable(
  "spo2_samples",
  {
    id: uuid("id").primaryKey().defaultRandom(),
    userId: uuid("user_id")
      .notNull()
      .references(() => users.id, { onDelete: "cascade" }),
    sourceBundleId: uuid("source_bundle_id").references(() => uploadBundles.id, {
      onDelete: "set null",
    }),
    recordedAt: timestamp("recorded_at", { withTimezone: true }).notNull(),
    spo2: doublePrecision("spo2"),
    raw: jsonb("raw"),
  },
  (t) => ({
    userTsUnique: uniqueIndex("spo2_samples_user_ts_uq").on(t.userId, t.recordedAt),
    userTsIdx: index("spo2_samples_user_ts_idx").on(t.userId, t.recordedAt),
  }),
)

/**
 * Daily strain rollup (movement + cardiovascular load for the calendar day).
 * Curated projection pushed by the device after its own on-device compute;
 * keyed by (user, day) so a re-push of the same day is an idempotent upsert.
 */
export const dailyStrain = pgTable(
  "daily_strain",
  {
    id: uuid("id").primaryKey().defaultRandom(),
    userId: uuid("user_id")
      .notNull()
      .references(() => users.id, { onDelete: "cascade" }),
    sourceBundleId: uuid("source_bundle_id").references(() => uploadBundles.id, {
      onDelete: "set null",
    }),
    day: date("day").notNull(),
    strainScore: doublePrecision("strain_score"),
    kilojoules: doublePrecision("kilojoules"),
    avgHrBpm: doublePrecision("avg_hr_bpm"),
    maxHrBpm: doublePrecision("max_hr_bpm"),
    // Free-text provenance of the curated row (e.g. "device_nightly_compute").
    source: text("source"),
    raw: jsonb("raw"),
    createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
    updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow(),
  },
  (t) => ({
    userDayUnique: uniqueIndex("daily_strain_user_day_uq").on(t.userId, t.day),
  }),
)

/**
 * Daily stress rollup. Same curated-projection contract as dailyStrain.
 */
export const dailyStress = pgTable(
  "daily_stress",
  {
    id: uuid("id").primaryKey().defaultRandom(),
    userId: uuid("user_id")
      .notNull()
      .references(() => users.id, { onDelete: "cascade" }),
    sourceBundleId: uuid("source_bundle_id").references(() => uploadBundles.id, {
      onDelete: "set null",
    }),
    day: date("day").notNull(),
    stressScore: doublePrecision("stress_score"),
    avgStress: doublePrecision("avg_stress"),
    maxStress: doublePrecision("max_stress"),
    highStressMinutes: doublePrecision("high_stress_minutes"),
    source: text("source"),
    raw: jsonb("raw"),
    createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
    updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow(),
  },
  (t) => ({
    userDayUnique: uniqueIndex("daily_stress_user_day_uq").on(t.userId, t.day),
  }),
)

/**
 * Daily energy / battery rollup. Same curated-projection contract.
 */
export const dailyEnergy = pgTable(
  "daily_energy",
  {
    id: uuid("id").primaryKey().defaultRandom(),
    userId: uuid("user_id")
      .notNull()
      .references(() => users.id, { onDelete: "cascade" }),
    sourceBundleId: uuid("source_bundle_id").references(() => uploadBundles.id, {
      onDelete: "set null",
    }),
    day: date("day").notNull(),
    energyScore: doublePrecision("energy_score"),
    energyBank: doublePrecision("energy_bank"),
    chargeRate: doublePrecision("charge_rate"),
    drainRate: doublePrecision("drain_rate"),
    source: text("source"),
    raw: jsonb("raw"),
    createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
    updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow(),
  },
  (t) => ({
    userDayUnique: uniqueIndex("daily_energy_user_day_uq").on(t.userId, t.day),
  }),
)

/**
 * Daily vitals rollup (resting HR, HRV, respiratory rate, skin temp, SpO2).
 * Each value originates from the connected device's own live sensor data.
 * Same curated-projection contract: keyed by (user, day), idempotent upsert.
 */
export const vitalsDaily = pgTable(
  "vitals_daily",
  {
    id: uuid("id").primaryKey().defaultRandom(),
    userId: uuid("user_id")
      .notNull()
      .references(() => users.id, { onDelete: "cascade" }),
    sourceBundleId: uuid("source_bundle_id").references(() => uploadBundles.id, {
      onDelete: "set null",
    }),
    day: date("day").notNull(),
    restingHrBpm: doublePrecision("resting_hr_bpm"),
    hrvMs: doublePrecision("hrv_ms"),
    respiratoryRate: doublePrecision("respiratory_rate"),
    skinTempC: doublePrecision("skin_temp_c"),
    spo2Pct: doublePrecision("spo2_pct"),
    source: text("source"),
    raw: jsonb("raw"),
    createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
    updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow(),
  },
  (t) => ({
    userDayUnique: uniqueIndex("vitals_daily_user_day_uq").on(t.userId, t.day),
  }),
)

export type User = typeof users.$inferSelect
export type AppleIdentity = typeof appleIdentities.$inferSelect
export type Device = typeof devices.$inferSelect
export type UploadBundle = typeof uploadBundles.$inferSelect
export type DailyRecovery = typeof dailyRecovery.$inferSelect
export type DailySleep = typeof dailySleep.$inferSelect
export type Spo2Sample = typeof spo2Samples.$inferSelect
export type DailyStrain = typeof dailyStrain.$inferSelect
export type DailyStress = typeof dailyStress.$inferSelect
export type DailyEnergy = typeof dailyEnergy.$inferSelect
export type VitalsDaily = typeof vitalsDaily.$inferSelect
