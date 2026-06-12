/**
 * Object storage for raw upload bundles (S3-compatible; Cloudflare R2).
 *
 * Raw bytes live in the bucket, keyed by user + content hash; Postgres keeps
 * only the metadata + a `storage_key` reference. Downloads are served as
 * short-lived presigned URLs so file bytes never proxy through the API.
 *
 * The store is exposed behind an interface so request handlers and tests can
 * inject a fake without touching the network.
 */

import { S3Client } from "bun"
import type { Env } from "./env.ts"
import { hasObjectStore } from "./env.ts"

export interface ObjectStore {
  /** Store bytes under key. Idempotent: re-putting the same key is safe. */
  put(key: string, bytes: Uint8Array, contentType: string): Promise<void>
  /** Short-lived presigned GET URL for downloading the object. */
  presignGet(key: string, expiresInSeconds: number): string
}

let cached: { store: ObjectStore; signature: string } | null = null

function r2Signature(env: Env): string {
  return [env.S3_ENDPOINT, env.S3_BUCKET, env.S3_REGION, env.S3_ACCESS_KEY_ID].join("|")
}

/** Returns the configured object store, or null when S3/R2 env is absent. */
export function getObjectStore(env: Env): ObjectStore | null {
  if (!hasObjectStore(env)) return null
  const signature = r2Signature(env)
  if (cached && cached.signature === signature) return cached.store

  // hasObjectStore() above guarantees these are present.
  const client = new S3Client({
    accessKeyId: env.S3_ACCESS_KEY_ID!,
    secretAccessKey: env.S3_SECRET_ACCESS_KEY!,
    bucket: env.S3_BUCKET!,
    endpoint: env.S3_ENDPOINT!,
    region: env.S3_REGION,
  })

  const store: ObjectStore = {
    async put(key, bytes, contentType) {
      await client.write(key, bytes, { type: contentType })
    },
    presignGet(key, expiresInSeconds) {
      return client.presign(key, { method: "GET", expiresIn: expiresInSeconds })
    },
  }
  cached = { store, signature }
  return store
}

/** Deterministic object key for a user's bundle. */
export function bundleObjectKey(userId: string, checksum: string): string {
  return `users/${userId}/bundles/${checksum}.bundle`
}
