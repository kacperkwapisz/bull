import { afterAll, beforeAll, describe, expect, test } from "bun:test"
import { rm } from "node:fs/promises"
import type { Env } from "../src/lib/env.ts"
import { ensureSchema, getDb } from "../src/db/client.ts"
import { upsertUserFromApple } from "../src/services/accounts.ts"
import { bundleStorageRoot, ingestBundle } from "../src/services/bundle-ingest.ts"
import { dataSummary, listRecovery, listUploads } from "../src/services/data-read.ts"

const TEST_DB = process.env.TEST_DATABASE_URL

// These exercise the real Postgres path. Without a database they are skipped so
// the unit suite stays green locally; CI provides TEST_DATABASE_URL.
const maybe = TEST_DB ? describe : describe.skip

maybe("data ingest roundtrip (Postgres)", () => {
  const env = {
    DATABASE_URL: TEST_DB,
    APPLE_ISSUER: "https://appleid.apple.com",
    APPLE_BUNDLE_ID: "com.bull.swift",
  } as unknown as Env

  beforeAll(async () => {
    await ensureSchema(env)
  })

  afterAll(async () => {
    await rm(bundleStorageRoot(), { recursive: true, force: true })
  })

  test("upsert account, ingest summary, read back", async () => {
    const db = getDb(env)
    expect(db).not.toBeNull()
    if (!db) return

    const sub = `apple-${crypto.randomUUID()}`
    const account = await upsertUserFromApple(db, { sub, isPrivateEmail: false }, "device-abc123")
    expect(account.created).toBe(true)

    const again = await upsertUserFromApple(db, { sub, isPrivateEmail: false })
    expect(again.userId).toBe(account.userId)
    expect(again.created).toBe(false)

    const bytes = new TextEncoder().encode("raw-export-bundle-bytes")
    const result = await ingestBundle(db, {
      userId: account.userId,
      deviceId: "device-abc123",
      bytes,
      summary: {
        recovery: [{ day: "2026-06-10", recovery_score: 71, hrv_ms: 64, resting_hr_bpm: 52 }],
        sleep: [{ day: "2026-06-10", sleep_score: 88, total_sleep_minutes: 451 }],
        spo2: [{ recorded_at: "2026-06-10T03:00:00.000Z", spo2: 96 }],
      },
    })
    expect(result.status).toBe("parsed")
    expect(result.deduped).toBe(false)

    // Idempotent re-upload of identical bytes dedupes.
    const dup = await ingestBundle(db, { userId: account.userId, bytes })
    expect(dup.deduped).toBe(true)
    expect(dup.bundleId).toBe(result.bundleId)

    const summary = await dataSummary(db, account.userId)
    expect(summary.recovery_days).toBe(1)
    expect(summary.sleep_days).toBe(1)
    expect(summary.spo2_samples).toBe(1)
    expect(summary.uploads).toBe(1)
    expect(summary.latest_recovery_day).toBe("2026-06-10")

    const recovery = await listRecovery(db, account.userId, { limit: 10 })
    expect(recovery[0]?.recoveryScore).toBe(71)

    const uploads = await listUploads(db, account.userId, 10)
    expect(uploads[0]?.status).toBe("parsed")
    expect(uploads[0]?.checksum).toBe(result.checksum)
  })
})
