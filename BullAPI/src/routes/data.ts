import { Hyper, ok, created, route } from "@hyper/core"
import { authJwt } from "@hyper/auth-jwt"
import { z } from "zod"
import type { Env } from "../lib/env.ts"
import { getDb } from "../db/client.ts"
import { getObjectStore } from "../lib/object-store.ts"
import { bundleSummarySchema, ingestBundle } from "../services/bundle-ingest.ts"
import { parseBundle, parseAllPending } from "../services/parse-bundle.ts"
import { profilePushSchema, upsertProfile } from "../services/profile.ts"
import { pushTokenSchema, upsertPushToken } from "../services/push-tokens.ts"
import { getSyncStatus } from "../services/sync-status.ts"
import {
  computeJournalInsights,
  journalUpsertSchema,
  listJournalEntries,
  upsertJournalEntry,
} from "../services/journal.ts"
import { JOURNAL_CATALOG } from "../services/journal-catalog.ts"
import { isQueryableMethod, runDataQuery, clearCachedSleepScores } from "../services/data-query.ts"
import {
  dataSummary,
  fetchCalendar,
  fetchHome,
  getBundleForUser,
  getInputReports,
  listEnergy,
  listRecovery,
  listSleep,
  listSpo2,
  listStrain,
  listStress,
  listUploads,
  listVitals,
} from "../services/data-read.ts"

const DOWNLOAD_URL_TTL_SECONDS = 15 * 60

// Sized for compressed overnight spool archives uploaded by the app; raw
// JSONL compresses ~10x, so a heavy night stays well under this.
const MAX_BUNDLE_BYTES = 128 * 1024 * 1024 // 128 MB

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  })
}

const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i

// Data routes are scoped to real accounts: the session token must carry the
// `user_id` UUID claim issued by /v1/auth/apple. No `sub` fallback.
function userIdFrom(ctx: { jwt?: { [claim: string]: unknown } }): string | null {
  const claim = ctx.jwt?.["user_id"]
  return typeof claim === "string" && UUID_RE.test(claim) ? claim : null
}

const listQuery = z.object({
  from: z.string().regex(/^\d{4}-\d{2}-\d{2}$/).optional(),
  to: z.string().regex(/^\d{4}-\d{2}-\d{2}$/).optional(),
  limit: z.coerce.number().int().min(1).max(1000).default(200),
})

export function dataRoutes(env: Env) {
  const jwt = authJwt({ secret: env.JWT_SECRET })

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
      const sourceField = form.get("source")
      const source = typeof sourceField === "string" && sourceField.length > 0 ? sourceField : undefined
      const packetCountField = form.get("packet_count")
      const packetCount = typeof packetCountField === "string" ? Number(packetCountField) : undefined
      const retryCountField = form.get("retry_count")
      const retryCount = typeof retryCountField === "string" ? Number(retryCountField) : undefined

      const result = await ingestBundle(db, store, {
        userId,
        ...(deviceId !== undefined ? { deviceId } : {}),
        bytes,
        contentType,
        ...(summary !== undefined ? { summary } : {}),
        ...(source !== undefined ? { source } : {}),
        ...(typeof packetCount === "number" && Number.isFinite(packetCount) ? { packetCount } : {}),
        ...(typeof retryCount === "number" && Number.isFinite(retryCount) ? { retryCount } : {}),
      })

      // Thin-client compute: drain pending bundles server-side without blocking
      // the upload response. Fire-and-forget + batched (one compute per user per
      // cycle) + globally serialized, so a flood of tiny bundles doesn't trigger
      // a full pipeline each. No-op unless the sidecar is configured.
      if (env.BULL_CORE_BIN && env.BULL_CORE_DATA_DIR) {
        const binaryPath = env.BULL_CORE_BIN
        const dataDir = env.BULL_CORE_DATA_DIR
        void parseAllPending(db, store, { binaryPath, dataDir, env }).catch((error: unknown) => {
          console.error("[parse] drain failed", {
            error: error instanceof Error ? error.message : String(error),
          })
        })
      }

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

  // BFF: single round-trip for everything the home + health screens need.
  // Optional ?date=yyyy-MM-dd pins scores to that calendar day.
  const home = route
    .get("/v1/data/home")
    .query(z.object({ date: z.string().regex(/^\d{4}-\d{2}-\d{2}$/).optional() }))
    .use(jwt)
    .handle(async ({ ctx, query }) => {
      const db = getDb(env)
      if (!db) return json(503, { error: "persistence_unavailable" })
      const userId = userIdFrom(ctx)
      if (!userId) return json(403, { error: "user_scope_required" })
      return ok(await fetchHome(db, userId, query.date))
    })

  // Calendar: full month of daily score summaries for the date picker.
  const calendar = route
    .get("/v1/data/calendar")
    .query(z.object({ month: z.string().regex(/^\d{4}-\d{2}$/) }))
    .use(jwt)
    .handle(async ({ ctx, query }) => {
      const db = getDb(env)
      if (!db) return json(503, { error: "persistence_unavailable" })
      const userId = userIdFrom(ctx)
      if (!userId) return json(403, { error: "user_scope_required" })
      return ok(await fetchCalendar(db, userId, query.month))
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

  const syncStatus = route
    .get("/v1/data/sync-status")
    .use(jwt)
    .handle(async ({ ctx }) => {
      const db = getDb(env)
      if (!db) return json(503, { error: "persistence_unavailable" })
      const userId = userIdFrom(ctx)
      if (!userId) return json(403, { error: "user_scope_required" })
      return ok(await getSyncStatus(db, userId))
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

  // Push: idempotent curated daily rows computed on-device. Independent of the
  const strain = route
    .get("/v1/data/strain")
    .query(listQuery)
    .use(jwt)
    .handle(async ({ ctx, query }) => {
      const db = getDb(env)
      if (!db) return json(503, { error: "persistence_unavailable" })
      const userId = userIdFrom(ctx)
      if (!userId) return json(403, { error: "user_scope_required" })
      const rows = await listStrain(db, userId, {
        ...(query.from !== undefined ? { from: query.from } : {}),
        ...(query.to !== undefined ? { to: query.to } : {}),
        limit: query.limit,
      })
      return ok({ rows })
    })

  const stress = route
    .get("/v1/data/stress")
    .query(listQuery)
    .use(jwt)
    .handle(async ({ ctx, query }) => {
      const db = getDb(env)
      if (!db) return json(503, { error: "persistence_unavailable" })
      const userId = userIdFrom(ctx)
      if (!userId) return json(403, { error: "user_scope_required" })
      const rows = await listStress(db, userId, {
        ...(query.from !== undefined ? { from: query.from } : {}),
        ...(query.to !== undefined ? { to: query.to } : {}),
        limit: query.limit,
      })
      return ok({ rows })
    })

  const energy = route
    .get("/v1/data/energy")
    .query(listQuery)
    .use(jwt)
    .handle(async ({ ctx, query }) => {
      const db = getDb(env)
      if (!db) return json(503, { error: "persistence_unavailable" })
      const userId = userIdFrom(ctx)
      if (!userId) return json(403, { error: "user_scope_required" })
      const rows = await listEnergy(db, userId, {
        ...(query.from !== undefined ? { from: query.from } : {}),
        ...(query.to !== undefined ? { to: query.to } : {}),
        limit: query.limit,
      })
      return ok({ rows })
    })

  const vitals = route
    .get("/v1/data/vitals")
    .query(listQuery)
    .use(jwt)
    .handle(async ({ ctx, query }) => {
      const db = getDb(env)
      if (!db) return json(503, { error: "persistence_unavailable" })
      const userId = userIdFrom(ctx)
      if (!userId) return json(403, { error: "user_scope_required" })
      const rows = await listVitals(db, userId, {
        ...(query.from !== undefined ? { from: query.from } : {}),
        ...(query.to !== undefined ? { to: query.to } : {}),
        limit: query.limit,
      })
      return ok({ rows })
    })

  // Profile upload: the user's own weight/DOB/sex + device timezone, so
  // server-side compute can derive energy and bucket daily rollups on the
  // user's local calendar day. Upsert; re-uploading replaces.
  const profilePush = route
    .post("/v1/data/profile")
    .body(profilePushSchema)
    .use(jwt)
    .handle(async ({ ctx, body }) => {
      const db = getDb(env)
      if (!db) return json(503, { error: "persistence_unavailable" })
      const userId = userIdFrom(ctx)
      if (!userId) return json(403, { error: "user_scope_required" })
      await upsertProfile(db, userId, body)
      return created({ ok: true })
    })

  // Register a device's APNs token so the server can push recovery-ready alerts.
  // Upsert by (user, token); environment routes the sender to sandbox vs prod.
  const pushToken = route
    .post("/v1/data/push-token")
    .body(pushTokenSchema)
    .use(jwt)
    .handle(async ({ ctx, body }) => {
      const db = getDb(env)
      if (!db) return json(503, { error: "persistence_unavailable" })
      const userId = userIdFrom(ctx)
      if (!userId) return json(403, { error: "user_scope_required" })
      await upsertPushToken(db, userId, body)
      return created({ ok: true })
    })

  // Packet-derived input reports (the whole dashboard input layer) computed
  // server-side. One latest map per user; honest-empty until first compute.
  const inputs = route
    .get("/v1/data/inputs")
    .use(jwt)
    .handle(async ({ ctx }) => {
      const db = getDb(env)
      if (!db) return json(503, { error: "persistence_unavailable" })
      const userId = userIdFrom(ctx)
      if (!userId) return json(403, { error: "user_scope_required" })
      const row = await getInputReports(db, userId)
      return ok({ reports: row?.raw ?? {}, computed_at: row?.computedAt ?? null })
    })

  // Read-through proxy: run a whitelisted read-only bull-core method against the
  // user's server-side store so display surfaces (nightly sleep, biometric
  // streams, recorded activity) read from the server instead of a local store.
  const dataQuery = route
    .post("/v1/data/query")
    .body(
      z.object({
        method: z.string().min(1),
        args: z.record(z.string(), z.union([z.string(), z.number(), z.boolean()])).default({}),
      }),
    )
    .use(jwt)
    .handle(async ({ ctx, body }) => {
      const userId = userIdFrom(ctx)
      if (!userId) return json(403, { error: "user_scope_required" })
      if (!isQueryableMethod(body.method)) return json(400, { error: "method_not_allowed" })
      try {
        const result = await runDataQuery(env, userId, body.method, body.args)
        return ok({ result: result ?? null })
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error)
        if (message === "sidecar_unavailable") return json(503, { error: "compute_unavailable" })
        if (message.startsWith("method_not_allowed")) return json(400, { error: "method_not_allowed" })
        return json(500, { error: "query_failed" })
      }
    })

  // Journal: upsert one day's logged behaviors (+ optional note).
  const journalUpsert = route
    .post("/v1/data/journal")
    .body(journalUpsertSchema)
    .use(jwt)
    .handle(async ({ ctx, body }) => {
      const db = getDb(env)
      if (!db) return json(503, { error: "persistence_unavailable" })
      const userId = userIdFrom(ctx)
      if (!userId) return json(403, { error: "user_scope_required" })
      await upsertJournalEntry(db, userId, body)
      return created({ ok: true })
    })

  // Journal history for the entry UI.
  const journalList = route
    .get("/v1/data/journal")
    .query(listQuery)
    .use(jwt)
    .handle(async ({ ctx, query }) => {
      const db = getDb(env)
      if (!db) return json(503, { error: "persistence_unavailable" })
      const userId = userIdFrom(ctx)
      if (!userId) return json(403, { error: "user_scope_required" })
      return ok({
        rows: await listJournalEntries(db, userId, {
          ...(query.from !== undefined ? { from: query.from } : {}),
          ...(query.to !== undefined ? { to: query.to } : {}),
          limit: query.limit,
        }),
      })
    })

  // Default behavior catalog for the picker (static; users add custom tags too).
  const journalCatalog = route
    .get("/v1/data/journal/catalog")
    .use(jwt)
    .handle(async () => ok({ tags: JOURNAL_CATALOG }))

  // Behavior → metric insights (correlation, not causation), computed in core.
  const journalInsights = route
    .get("/v1/data/journal/insights")
    .query(z.object({ metric: z.enum(["recovery", "sleep"]).default("recovery") }))
    .use(jwt)
    .handle(async ({ ctx, query }) => {
      const db = getDb(env)
      if (!db) return json(503, { error: "persistence_unavailable" })
      const userId = userIdFrom(ctx)
      if (!userId) return json(403, { error: "user_scope_required" })
      const result = await computeJournalInsights(env, db, userId, query.metric)
      if (result === null) return json(503, { error: "compute_unavailable" })
      return ok({ insights: result })
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

  // Server-side parse: run bull-core over a pending bundle (thin-client).
  const parse = route
    .post("/v1/data/uploads/:id/parse")
    .use(jwt)
    .handle(async ({ ctx, params }) => {
      const db = getDb(env)
      if (!db) return json(503, { error: "persistence_unavailable" })
      const store = getObjectStore(env)
      if (!store) return json(503, { error: "storage_unavailable" })
      if (!env.BULL_CORE_BIN || !env.BULL_CORE_DATA_DIR) {
        return json(503, { error: "parse_unavailable" })
      }
      const userId = userIdFrom(ctx)
      if (!userId) return json(403, { error: "user_scope_required" })
      const id = (params as { id?: string }).id
      if (!id) return json(400, { error: "missing_bundle_id" })
      const result = await parseBundle(
        db,
        store,
        { binaryPath: env.BULL_CORE_BIN, dataDir: env.BULL_CORE_DATA_DIR, env },
        userId,
        id,
      )
      if (result.status === "failed") return json(422, result)
      return ok(result)
    })

  const sleepRecalculate = route
    .post("/v1/data/sleep/recalculate")
    .use(jwt)
    .handle(async ({ ctx }) => {
      const userId = userIdFrom(ctx)
      if (!userId) return json(403, { error: "user_scope_required" })
      try {
        const result = await clearCachedSleepScores(env, userId)
        if (result === null) return ok({ cleared: false, reason: "no_store" })
        return ok({ cleared: true, ...(result as object) })
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error)
        if (message === "sidecar_unavailable") return json(503, { error: "compute_unavailable" })
        return json(500, { error: "clear_failed" })
      }
    })

  return new Hyper({ prefix: "" }).use([
    upload,
    download,
    parse,
    home,
    calendar,
    summary,
    syncStatus,
    recovery,
    sleep,
    sleepRecalculate,
    strain,
    stress,
    energy,
    vitals,
    spo2,
    profilePush,
    pushToken,
    inputs,
    journalUpsert,
    journalList,
    journalCatalog,
    journalInsights,
    dataQuery,
    uploads,
  ])
}
