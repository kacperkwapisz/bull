import { z } from "zod"

const envSchema = z.object({
  PORT: z.coerce.number().default(3000),
  JWT_SECRET: z.string().min(32),
  BULL_UPSTREAM_BASE_URL: z.string().url().default("https://oraiapi.com/v1"),
  BULL_UPSTREAM_API_KEY: z.string().min(1),
  BULL_MODEL_DEFAULT: z.string().default("gpt-oss-120b"),
  BULL_MODEL_DEEP: z.string().default("gpt-oss-120b"),
  BULL_DEV_AUTH_BYPASS: z
    .enum(["0", "1"])
    .default("0")
    .transform((v) => v === "1"),
  CORS_ORIGINS: z.string().optional(),
  // Persistence. Optional so coach-only / test runs boot without a database;
  // data + Apple-account routes return 503 when it is absent.
  DATABASE_URL: z.string().url().optional(),
  // Sign in with Apple. APPLE_BUNDLE_ID is the audience BullAPI requires in
  // Apple identity tokens; APPLE_ISSUER is fixed and overridable only for tests.
  APPLE_BUNDLE_ID: z.string().min(1).default("com.bull.swift"),
  APPLE_ISSUER: z.string().url().default("https://appleid.apple.com"),
  // Object storage (S3-compatible; Cloudflare R2) for raw upload bundles.
  // Optional so coach-only / test runs boot without it; the upload route
  // returns 503 when storage is unconfigured. All five are required together.
  S3_ENDPOINT: z.string().url().optional(),
  S3_BUCKET: z.string().min(1).optional(),
  S3_REGION: z.string().min(1).default("auto"),
  S3_ACCESS_KEY_ID: z.string().min(1).optional(),
  S3_SECRET_ACCESS_KEY: z.string().min(1).optional(),
})

export type Env = z.infer<typeof envSchema>

export function loadEnv(): Env {
  // Treat empty-string vars as absent so a partially-filled environment (e.g. a
  // placeholder `.env` with blank S3 keys) doesn't block coach-only boot.
  const cleaned = Object.fromEntries(
    Object.entries(process.env).filter(([, v]) => v !== ""),
  )
  const parsed = envSchema.safeParse(cleaned)
  if (!parsed.success) {
    const message = parsed.error.issues.map((i) => `${i.path.join(".")}: ${i.message}`).join("; ")
    throw new Error(`BullAPI env invalid: ${message}`)
  }
  return parsed.data
}

/** True when all required S3/R2 settings are present. */
export function hasObjectStore(env: Env): boolean {
  return Boolean(
    env.S3_ENDPOINT && env.S3_BUCKET && env.S3_ACCESS_KEY_ID && env.S3_SECRET_ACCESS_KEY,
  )
}

export function corsOrigins(env: Env): readonly string[] | "*" {
  const raw = env.CORS_ORIGINS?.trim()
  if (!raw || raw === "*") {
    return "*"
  }
  return raw.split(",").map((o) => o.trim()).filter(Boolean)
}