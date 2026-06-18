import { z } from "zod"
import { sql } from "drizzle-orm"
import type { Db } from "../db/client.ts"
import { pushTokens } from "../db/schema.ts"

export const pushTokenSchema = z.object({
  token: z.string().min(1),
  platform: z.string().min(1).default("ios"),
  environment: z.enum(["sandbox", "production"]).default("production"),
  bundle_id: z.string().min(1).optional(),
})

export type PushTokenInput = z.infer<typeof pushTokenSchema>

/** Upsert a device's APNs token, keyed by (user, token). */
export async function upsertPushToken(
  db: Db,
  userId: string,
  input: PushTokenInput,
): Promise<void> {
  await db
    .insert(pushTokens)
    .values({
      userId,
      token: input.token,
      platform: input.platform,
      environment: input.environment,
      bundleId: input.bundle_id ?? null,
    })
    .onConflictDoUpdate({
      target: [pushTokens.userId, pushTokens.token],
      set: {
        platform: input.platform,
        environment: input.environment,
        bundleId: input.bundle_id ?? null,
        updatedAt: sql`now()`,
      },
    })
}
