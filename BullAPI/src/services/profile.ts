/**
 * Per-user profile + timezone upload.
 *
 * The app uploads the connected user's own profile (weight, date of birth, sex)
 * and device timezone so server-side compute can derive energy/calorie estimates
 * and bucket daily rollups on the user's local calendar day. Every field is
 * optional; when absent, compute degrades honestly rather than guessing.
 */

import { z } from "zod"
import { eq } from "drizzle-orm"
import type { Db } from "../db/client.ts"
import { userProfiles } from "../db/schema.ts"

export const profilePushSchema = z.object({
  weight_grams: z.number().int().positive().nullish(),
  date_of_birth: z
    .string()
    .regex(/^\d{4}-\d{2}-\d{2}$/)
    .nullish(),
  sex: z.enum(["male", "female"]).nullish(),
  timezone: z.string().min(1).max(64).nullish(),
})

export type ProfilePush = z.infer<typeof profilePushSchema>

/** Upsert the user's profile (one row per user). */
export async function upsertProfile(db: Db, userId: string, body: ProfilePush): Promise<void> {
  const values = {
    userId,
    weightGrams: body.weight_grams ?? null,
    dateOfBirth: body.date_of_birth ?? null,
    sex: body.sex ?? null,
    timezone: body.timezone ?? null,
    updatedAt: new Date(),
  }
  await db
    .insert(userProfiles)
    .values(values)
    .onConflictDoUpdate({
      target: userProfiles.userId,
      set: {
        weightGrams: values.weightGrams,
        dateOfBirth: values.dateOfBirth,
        sex: values.sex,
        timezone: values.timezone,
        updatedAt: values.updatedAt,
      },
    })
}

export interface UserProfile {
  weightGrams: number | null
  dateOfBirth: string | null
  sex: string | null
  timezone: string | null
}

/** The user's profile, or null if none uploaded yet. */
export async function getUserProfile(db: Db, userId: string): Promise<UserProfile | null> {
  const rows = await db
    .select({
      weightGrams: userProfiles.weightGrams,
      dateOfBirth: userProfiles.dateOfBirth,
      sex: userProfiles.sex,
      timezone: userProfiles.timezone,
    })
    .from(userProfiles)
    .where(eq(userProfiles.userId, userId))
    .limit(1)
  return rows[0] ?? null
}
