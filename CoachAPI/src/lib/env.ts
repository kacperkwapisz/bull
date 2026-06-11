import { z } from "zod"

const envSchema = z.object({
  PORT: z.coerce.number().default(3000),
  JWT_SECRET: z.string().min(32),
  COACH_UPSTREAM_BASE_URL: z.string().url().default("https://oraiapi.com/v1"),
  COACH_UPSTREAM_API_KEY: z.string().min(1),
  COACH_MODEL_DEFAULT: z.string().default("gpt-oss-120b"),
  COACH_MODEL_DEEP: z.string().default("gpt-oss-120b"),
  COACH_DEV_AUTH_BYPASS: z
    .enum(["0", "1"])
    .default("0")
    .transform((v) => v === "1"),
  CORS_ORIGINS: z.string().optional(),
})

export type Env = z.infer<typeof envSchema>

export function loadEnv(): Env {
  const parsed = envSchema.safeParse(process.env)
  if (!parsed.success) {
    const message = parsed.error.issues.map((i) => `${i.path.join(".")}: ${i.message}`).join("; ")
    throw new Error(`CoachAPI env invalid: ${message}`)
  }
  return parsed.data
}

export function corsOrigins(env: Env): readonly string[] | "*" {
  const raw = env.CORS_ORIGINS?.trim()
  if (!raw || raw === "*") {
    return "*"
  }
  return raw.split(",").map((o) => o.trim()).filter(Boolean)
}