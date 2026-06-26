/**
 * Client for the `bull-bridge-serve` sidecar — the same `bull-core` parser the
 * device runs, invoked server-side. It speaks newline-delimited JSON: one
 * BridgeRequest per line in, one BridgeResponse per line out, in order.
 *
 * One sidecar process is spawned per parse operation and kept alive for the
 * sequence of calls (import → run_pipeline → reads), then closed.
 */

const BRIDGE_REQUEST_SCHEMA = "bull.bridge.request.v1"

export interface BullCoreResponse<T = unknown> {
  ok: boolean
  result?: T
  error?: { code: string; message: string }
}

export class BullCoreError extends Error {
  constructor(
    readonly method: string,
    readonly code: string,
    message: string,
  ) {
    super(`bull-core ${method} failed (${code}): ${message}`)
    this.name = "BullCoreError"
  }
}

// Cap on retained sidecar stderr so a chatty crash can't grow unbounded in
// memory; a backtrace easily fits and we keep the most recent bytes.
const MAX_STDERR_BYTES = 16_384

export class BullCore {
  private readonly proc: ReturnType<typeof Bun.spawn>
  private readonly reader: ReadableStreamDefaultReader<Uint8Array>
  private readonly decoder = new TextDecoder()
  private buffer = ""
  private requestId = 0
  /** Most-recent stderr from the sidecar (RUST_BACKTRACE + panic/abort text). */
  private stderr = ""

  constructor(binaryPath: string) {
    this.proc = Bun.spawn([binaryPath], {
      stdin: "pipe",
      stdout: "pipe",
      // Capture stderr instead of inheriting it: a hard crash (panic, abort,
      // stack overflow, SIGSEGV) writes its diagnosis here, and we fold that
      // into the thrown error so the real cause reaches parse_error instead of
      // a useless "closed unexpectedly". RUST_BACKTRACE makes panics carry a
      // full backtrace.
      stderr: "pipe",
      env: { ...process.env, RUST_BACKTRACE: process.env.RUST_BACKTRACE ?? "1" },
    })
    this.reader = (this.proc.stdout as ReadableStream<Uint8Array>).getReader()
    this.drainStderr()
  }

  /** Continuously accumulate the sidecar's stderr (bounded) so it is available
   * when the process dies. */
  private async drainStderr(): Promise<void> {
    try {
      const reader = (this.proc.stderr as ReadableStream<Uint8Array>).getReader()
      const decoder = new TextDecoder()
      for (;;) {
        const { value, done } = await reader.read()
        if (done) break
        this.stderr += decoder.decode(value, { stream: true })
        if (this.stderr.length > MAX_STDERR_BYTES) {
          this.stderr = this.stderr.slice(-MAX_STDERR_BYTES)
        }
      }
    } catch {
      // stderr drain is best-effort diagnostics; never throw from it.
    }
  }

  /** Wait for the process to exit and describe how it died (code or signal). */
  private async exitDescription(): Promise<string> {
    let how = ""
    try {
      await this.proc.exited
      const code = this.proc.exitCode
      const signal = (this.proc as { signalCode?: string | null }).signalCode
      if (signal) how = `killed by signal ${signal}`
      else if (typeof code === "number") how = `exited with code ${code}`
    } catch {
      // ignore — we still surface whatever stderr we captured
    }
    // Give the stderr drain a tick to flush the final bytes.
    await new Promise((resolve) => setTimeout(resolve, 25))
    const tail = this.stderr.trim()
    const parts = [how, tail && `stderr: ${tail}`].filter(Boolean)
    return parts.length > 0 ? parts.join(" — ") : "no diagnostic output"
  }

  /** Call a bridge method; throws BullCoreError if the sidecar reports !ok.
   * Includes a 120s timeout per request to prevent hangs from dead sidecars. */
  async request<T = unknown>(method: string, args: unknown): Promise<T> {
    const id = String(++this.requestId)
    const payload =
      JSON.stringify({ schema: BRIDGE_REQUEST_SCHEMA, request_id: id, method, args }) + "\n"
    const stdin = this.proc.stdin as { write(s: string): void; flush?(): void }
    stdin.write(payload)
    stdin.flush?.()

    // run_pipeline does ~15 sub-steps scanning the full store; allow 5 min.
    const timeoutMs = method.includes("run_pipeline") ? 300_000 : 120_000
    const line = await this.readLineWithTimeout(timeoutMs, method)
    const response = JSON.parse(line) as BullCoreResponse<T>
    if (!response.ok) {
      const error = response.error ?? { code: "unknown", message: "no error detail" }
      throw new BullCoreError(method, error.code, error.message)
    }
    return response.result as T
  }

  private readLineWithTimeout(ms: number, method: string): Promise<string> {
    return new Promise((resolve, reject) => {
      let settled = false
      const timer = setTimeout(() => {
        if (settled) return
        settled = true
        this.close()
        this.exitDescription().then(
          (why) => reject(new Error(`bull-core sidecar timed out after ${ms}ms on ${method} (${why})`)),
          () => reject(new Error(`bull-core sidecar timed out after ${ms}ms on ${method}`)),
        )
      }, ms)
      this.readLine().then(
        (line) => {
          if (settled) return
          settled = true
          clearTimeout(timer)
          resolve(line)
        },
        (err) => {
          if (settled) return
          settled = true
          clearTimeout(timer)
          reject(err)
        },
      )
    })
  }

  private async readLine(): Promise<string> {
    while (!this.buffer.includes("\n")) {
      const { value, done } = await this.reader.read()
      if (done) {
        const why = await this.exitDescription()
        throw new Error(`bull-core sidecar closed unexpectedly (${why})`)
      }
      this.buffer += this.decoder.decode(value, { stream: true })
    }
    const index = this.buffer.indexOf("\n")
    const line = this.buffer.slice(0, index)
    this.buffer = this.buffer.slice(index + 1)
    return line
  }

  close(): void {
    try {
      ;(this.proc.stdin as { end?(): void }).end?.()
    } catch {
      // ignore
    }
    try {
      this.proc.kill()
    } catch {
      // ignore
    }
  }
}
