/**
 * Lazy Drizzle client over postgres.js for BullAPI.
 *
 * Persistence is optional: when DATABASE_URL is absent (coach-only runs / unit
 * tests) getDb() returns null and persistence-dependent routes answer 503
 * instead of crashing the service. The connection is created on first use,
 * never at import time.
 */

import { drizzle } from "drizzle-orm/postgres-js"
import { migrate } from "drizzle-orm/postgres-js/migrator"
import { sql } from "drizzle-orm"
import postgres from "postgres"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"
import type { Env } from "../lib/env.ts"
import * as schema from "./schema.ts"

export type Db = ReturnType<typeof drizzle<typeof schema>>

let pg: ReturnType<typeof postgres> | undefined
let db: Db | undefined
let connectedUrl: string | undefined

export function getDb(env: Env): Db | null {
  if (!env.DATABASE_URL) return null
  if (db && connectedUrl === env.DATABASE_URL) return db
  pg = postgres(env.DATABASE_URL, { max: 10 })
  db = drizzle(pg, { schema })
  connectedUrl = env.DATABASE_URL
  return db
}

function migrationsFolder(): string {
  return join(dirname(fileURLToPath(import.meta.url)), "migrations")
}

/**
 * Apply pending migrations. Production runs this via the Docker entrypoint
 * (`bun run db:migrate`); tests/local call it directly. Drizzle tracks applied
 * migrations, so it is idempotent.
 */
export async function ensureSchema(env: Env): Promise<void> {
  if (!env.DATABASE_URL) return
  const migrator = postgres(env.DATABASE_URL, { max: 1 })
  try {
    await migrate(drizzle(migrator), { migrationsFolder: migrationsFolder() })
  } finally {
    await migrator.end({ timeout: 5 })
  }
}

/** Lightweight connectivity probe for /health. */
export async function pingDb(env: Env): Promise<boolean> {
  const conn = getDb(env)
  if (!conn) return false
  try {
    await conn.execute(sql`select 1`)
    return true
  } catch {
    return false
  }
}

export async function closeDb(): Promise<void> {
  if (pg) {
    await pg.end({ timeout: 5 })
    pg = undefined
    db = undefined
    connectedUrl = undefined
  }
}
