/**
 * Apply pending Drizzle migrations. Run with `bun run db:migrate`.
 * Invoked by the Docker entrypoint before the server starts.
 */
import { drizzle } from "drizzle-orm/postgres-js"
import { migrate } from "drizzle-orm/postgres-js/migrator"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"
import postgres from "postgres"

const url = process.env.DATABASE_URL
if (!url) throw new Error("DATABASE_URL is not defined")

const folder = join(dirname(fileURLToPath(import.meta.url)), "migrations")
const connection = postgres(url, { max: 1 })
const db = drizzle(connection)

console.log("⏳ Running BullAPI migrations...")
const start = Date.now()
await migrate(db, { migrationsFolder: folder })
console.log(`✅ Migrations completed in ${Date.now() - start}ms`)
await connection.end()
process.exit(0)
