import { and, desc, eq, gte, inArray, lte } from "drizzle-orm"
import { z } from "zod"
import type { Db } from "../db/client.ts"
import { dailyRecovery, dailySleep, journalEntries } from "../db/schema.ts"
import type { Env } from "../lib/env.ts"
import { BullCore } from "../lib/bull-core.ts"
import type { DayRange } from "./data-read.ts"

const DAY_RE = /^\d{4}-\d{2}-\d{2}$/

const behaviorSchema = z.object({
  tag: z.string().min(1).max(64),
  amount: z.number().finite().optional(),
})

export const journalUpsertSchema = z.object({
  day: z.string().regex(DAY_RE),
  behaviors: z.array(behaviorSchema).max(128).default([]),
  note: z.string().max(2000).optional(),
})

export type JournalUpsertInput = z.infer<typeof journalUpsertSchema>

/** Insert or replace the journal entry for (user, day). */
export async function upsertJournalEntry(
  db: Db,
  userId: string,
  input: JournalUpsertInput,
): Promise<void> {
  const now = new Date()
  await db
    .insert(journalEntries)
    .values({
      userId,
      day: input.day,
      behaviors: input.behaviors,
      note: input.note ?? null,
      updatedAt: now,
    })
    .onConflictDoUpdate({
      target: [journalEntries.userId, journalEntries.day],
      set: {
        behaviors: input.behaviors,
        note: input.note ?? null,
        updatedAt: now,
      },
    })
}

/** List journal entries for a user, newest first. */
export async function listJournalEntries(db: Db, userId: string, range: DayRange) {
  const conds = [eq(journalEntries.userId, userId)]
  if (range.from) conds.push(gte(journalEntries.day, range.from))
  if (range.to) conds.push(lte(journalEntries.day, range.to))
  return db
    .select()
    .from(journalEntries)
    .where(and(...conds))
    .orderBy(desc(journalEntries.day))
    .limit(range.limit)
}

export type InsightMetric = "recovery" | "sleep"

/**
 * Compute behavior insights for a user over a trailing window.
 *
 * Builds one record per day that has the chosen metric score, attaching that
 * day's logged behaviors (or none), then runs the tested Rust engine via the
 * core sidecar. Returns `null` when the core binary isn't configured so the
 * caller can surface an honest unavailable state.
 */
export async function computeJournalInsights(
  env: Env,
  db: Db,
  userId: string,
  metric: InsightMetric,
  windowDays = 90,
): Promise<unknown | null> {
  if (!env.BULL_CORE_BIN) return null

  const since = new Date()
  since.setUTCDate(since.getUTCDate() - windowDays)
  const from = since.toISOString().slice(0, 10)

  // Metric scores keyed by day.
  const scoreByDay = new Map<string, number>()
  if (metric === "recovery") {
    const rows = await db
      .select({ day: dailyRecovery.day, score: dailyRecovery.recoveryScore })
      .from(dailyRecovery)
      .where(and(eq(dailyRecovery.userId, userId), gte(dailyRecovery.day, from)))
    for (const r of rows) if (r.score != null) scoreByDay.set(r.day, r.score)
  } else {
    const rows = await db
      .select({ day: dailySleep.day, score: dailySleep.sleepScore })
      .from(dailySleep)
      .where(and(eq(dailySleep.userId, userId), gte(dailySleep.day, from)))
    for (const r of rows) if (r.score != null) scoreByDay.set(r.day, r.score)
  }

  if (scoreByDay.size === 0) {
    return { metric, analyzed_days: 0, impacts: [], insufficient: [], correlation_only: true }
  }

  // Behaviors keyed by day.
  const days = [...scoreByDay.keys()]
  const journalRows = await db
    .select({ day: journalEntries.day, behaviors: journalEntries.behaviors })
    .from(journalEntries)
    .where(and(eq(journalEntries.userId, userId), inArray(journalEntries.day, days)))
  const tagsByDay = new Map<string, string[]>()
  for (const row of journalRows) {
    tagsByDay.set(
      row.day,
      (row.behaviors ?? []).map((b) => b.tag),
    )
  }

  const records = days.map((day) => ({
    date: day,
    score: scoreByDay.get(day) ?? null,
    behaviors: tagsByDay.get(day) ?? [],
  }))

  const core = new BullCore(env.BULL_CORE_BIN)
  try {
    return await core.request("behavior.insights", { records, metric })
  } finally {
    core.close()
  }
}
