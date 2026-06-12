import { Hyper, ok, created, route } from "@hyper/core"
import { authJwt } from "@hyper/auth-jwt"
import { z } from "zod"
import type { Env } from "../lib/env.ts"
import { getDb } from "../db/client.ts"
import { getObjectStore } from "../lib/object-store.ts"
import { bundleSummarySchema, ingestBundle } from "../services/bundle-ingest.ts"
import {
  dataSummary,
  getBundleForUser,
  listRecovery,
  listSleep,
  listSpo2,
  listUploads,
} from "../services/data-read.ts"

const DOWNLOAD_URL_TTL_SECONDS = 15 * 60

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
    .handle(async ({ body, ctx }) => {
      const db = getDb(env)
      if (!db) return json(503, { error: "persistence_unavailable" })
      const store = getObjectStore(env)
      if (!store) return json(503, { error: "storage_unavailable" })
      const userId = userIdFrom(ctx)
      if (!userId) return json(403, { error: "user_scope_required" })

      // Hyper pre-parses the request body; a multipart upload arrives as FormData.
      if (!(body instanceof FormData)) return json(400, { error: "expected_multipart_form" })
      const form = body
      const file = form.get("bundle")
      if (!(file instanceof File)) return json(400, { error: "missing_bundle_file" })
      if (file.size > MAX_BUNDLE_BYTES) return json(413, { error: "bundle_too_large" })
      const bytes = new Uint8Array(await file.arrayBuffer())
      if (bytes.byteLength === 0) return json(400, { error: "empty_bundle" })
      const contentType = file.type && file.type.length > 0 ? file.type : "application/octet-stream"

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

      const result = await ingestBundle(db, store, {
        userId,
        ...(deviceId !== undefined ? { deviceId } : {}),
        bytes,
        contentType,
        ...(summary !== undefined ? { summary } : {}),
      })
      return created(result)
    })

  // Presigned download of a raw bundle. Returns a short-lived URL; the file
  // bytes are served directly by object storage, never proxied through the API.
  const download = route
    .get("/v1/data/uploads/:id/download")
    .use(jwt)
    .handle(async ({ ctx, params }) => {
      const db = getDb(env)
      if (!db) return json(503, { error: "persistence_unavailable" })
      const store = getObjectStore(env)
      if (!store) return json(503, { error: "storage_unavailable" })
      const userId = userIdFrom(ctx)
      if (!userId) return json(403, { error: "user_scope_required" })
      const id = (params as { id?: string }).id
      if (!id) return json(400, { error: "missing_bundle_id" })
      const bundle = await getBundleForUser(db, userId, id)
      if (!bundle) return json(404, { error: "bundle_not_found" })
      const url = store.presignGet(bundle.storageKey, DOWNLOAD_URL_TTL_SECONDS)
      return ok({
        url,
        expires_in: DOWNLOAD_URL_TTL_SECONDS,
        checksum: bundle.checksum,
        byte_size: bundle.byteSize,
        content_type: bundle.contentType,
      })
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

  return new Hyper({ prefix: "" }).use([
    upload,
    download,
    summary,
    recovery,
    sleep,
    spo2,
    uploads,
  ])
}
