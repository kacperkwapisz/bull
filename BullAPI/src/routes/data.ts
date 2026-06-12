import { Hyper, ok, created, route } from "@hyper/core"
import { authJwt } from "@hyper/auth-jwt"
import { z } from "zod"
import type { Env } from "../lib/env.ts"
import { getDb } from "../db/client.ts"
import { bundleSummarySchema, ingestBundle } from "../services/bundle-ingest.ts"
import {
  dataSummary,
  listRecovery,
  listSleep,
  listSpo2,
  listUploads,
} from "../services/data-read.ts"

const MAX_BUNDLE_BYTES = 64 * 1024 * 1024 // 64 MB

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  })
}

function userIdFrom(ctx: { jwt?: { user_id?: unknown; sub?: unknown } }): string | null {
  const claim = ctx.jwt?.user_id ?? ctx.jwt?.sub
  return typeof claim === "string" && claim.length > 0 ? claim : null
}

const listQuery = z.object({
  from: z.string().regex(/^\d{4}-\d{2}-\d{2}$/).optional(),
  to: z.string().regex(/^\d{4}-\d{2}-\d{2}$/).optional(),
  limit: z.coerce.number().int().min(1).max(1000).default(200),
})

export function dataRoutes(env: Env) {
  const jwt = authJwt({
    secret: env.JWT_SECRET,
    allowShortSecret: env.BULL_DEV_AUTH_BYPASS,
  })

  // Ingest: store the raw export bundle (source of record) and, if a summary
  // is attached, project curated rows. Multipart form: `bundle` (file),
  // optional `summary` (JSON string), optional `device_id`.
  const upload = route
    .post("/v1/data/uploads")
    .use(jwt)
    .handle(async ({ req, ctx }) => {
      const db = getDb(env)
      if (!db) return json(503, { error: "persistence_unavailable" })
      const userId = userIdFrom(ctx)
      if (!userId) return json(403, { error: "user_scope_required" })

      let form: FormData
      try {
        form = await req.formData()
      } catch {
        return json(400, { error: "expected_multipart_form" })
      }
      const file = form.get("bundle")
      if (!(file instanceof File)) return json(400, { error: "missing_bundle_file" })
      if (file.size > MAX_BUNDLE_BYTES) return json(413, { error: "bundle_too_large" })
      const bytes = new Uint8Array(await file.arrayBuffer())
      if (bytes.byteLength === 0) return json(400, { error: "empty_bundle" })

      let summary
      const summaryRaw = form.get("summary")
      if (typeof summaryRaw === "string" && summaryRaw.trim().length > 0) {
        const parsed = bundleSummarySchema.safeParse(JSON.parse(summaryRaw))
        if (!parsed.success) {
          return json(400, { error: "invalid_summary", issues: parsed.error.issues })
        }
        summary = parsed.data
      }
      const deviceField = form.get("device_id")
      const deviceId = typeof deviceField === "string" ? deviceField : undefined

      const result = await ingestBundle(db, {
        userId,
        ...(deviceId !== undefined ? { deviceId } : {}),
        bytes,
        ...(summary !== undefined ? { summary } : {}),
      })
      return created(result)
    })

  const summary = route
    .get("/v1/data/summary")
    .use(jwt)
    .handle(async ({ ctx }) => {
      const db = getDb(env)
      if (!db) return json(503, { error: "persistence_unavailable" })
      const userId = userIdFrom(ctx)
      if (!userId) return json(403, { error: "user_scope_required" })
      return ok(await dataSummary(db, userId))
    })

  const recovery = route
    .get("/v1/data/recovery")
    .query(listQuery)
    .use(jwt)
    .handle(async ({ ctx, query }) => {
      const db = getDb(env)
      if (!db) return json(503, { error: "persistence_unavailable" })
      const userId = userIdFrom(ctx)
      if (!userId) return json(403, { error: "user_scope_required" })
      const rows = await listRecovery(db, userId, {
        ...(query.from !== undefined ? { from: query.from } : {}),
        ...(query.to !== undefined ? { to: query.to } : {}),
        limit: query.limit,
      })
      return ok({ rows })
    })

  const sleep = route
    .get("/v1/data/sleep")
    .query(listQuery)
    .use(jwt)
    .handle(async ({ ctx, query }) => {
      const db = getDb(env)
      if (!db) return json(503, { error: "persistence_unavailable" })
      const userId = userIdFrom(ctx)
      if (!userId) return json(403, { error: "user_scope_required" })
      const rows = await listSleep(db, userId, {
        ...(query.from !== undefined ? { from: query.from } : {}),
        ...(query.to !== undefined ? { to: query.to } : {}),
        limit: query.limit,
      })
      return ok({ rows })
    })

  const spo2 = route
    .get("/v1/data/spo2")
    .query(listQuery)
    .use(jwt)
    .handle(async ({ ctx, query }) => {
      const db = getDb(env)
      if (!db) return json(503, { error: "persistence_unavailable" })
      const userId = userIdFrom(ctx)
      if (!userId) return json(403, { error: "user_scope_required" })
      return ok({ rows: await listSpo2(db, userId, query.limit) })
    })

  const uploads = route
    .get("/v1/data/uploads")
    .query(listQuery)
    .use(jwt)
    .handle(async ({ ctx, query }) => {
      const db = getDb(env)
      if (!db) return json(503, { error: "persistence_unavailable" })
      const userId = userIdFrom(ctx)
      if (!userId) return json(403, { error: "user_scope_required" })
      return ok({ rows: await listUploads(db, userId, query.limit) })
    })

  return new Hyper({ prefix: "" }).use([upload, summary, recovery, sleep, spo2, uploads])
}
