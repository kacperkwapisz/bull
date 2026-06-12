/**
 * Account upsert from a verified Apple identity. One Bull user per Apple
 * subject; re-sign-in returns the same user. Optionally records the device.
 */

import { eq, sql } from "drizzle-orm"
import type { Db } from "../db/client.ts"
import { appleIdentities, devices, users } from "../db/schema.ts"
import type { AppleIdentity } from "../lib/apple-auth.ts"

export interface ResolvedAccount {
  readonly userId: string
  readonly created: boolean
}

export async function upsertUserFromApple(
  db: Db,
  identity: AppleIdentity,
  deviceId?: string,
): Promise<ResolvedAccount> {
  const existing = await db
    .select({ userId: appleIdentities.userId })
    .from(appleIdentities)
    .where(eq(appleIdentities.appleSub, identity.sub))
    .limit(1)

  let userId: string
  let created = false

  if (existing.length > 0 && existing[0]) {
    userId = existing[0].userId
    await db.update(users).set({ lastSeenAt: sql`now()` }).where(eq(users.id, userId))
  } else {
    const inserted = await db.insert(users).values({}).returning({ id: users.id })
    const row = inserted[0]
    if (!row) throw new Error("failed to create user")
    userId = row.id
    created = true
    await db.insert(appleIdentities).values({
      userId,
      appleSub: identity.sub,
      ...(identity.email !== undefined ? { email: identity.email } : {}),
      isPrivateEmail: identity.isPrivateEmail ? 1 : 0,
    })
  }

  if (deviceId) {
    await db
      .insert(devices)
      .values({ userId, deviceId })
      .onConflictDoUpdate({
        target: [devices.userId, devices.deviceId],
        set: { lastSeenAt: sql`now()` },
      })
  }

  return { userId, created }
}
