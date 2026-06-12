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

export type User = typeof users.$inferSelect
export type AppleIdentity = typeof appleIdentities.$inferSelect
export type Device = typeof devices.$inferSelect
export type UploadBundle = typeof uploadBundles.$inferSelect
export type DailyRecovery = typeof dailyRecovery.$inferSelect
export type DailySleep = typeof dailySleep.$inferSelect
export type Spo2Sample = typeof spo2Samples.$inferSelect
