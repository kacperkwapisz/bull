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

export class BullCore {
  private readonly proc: ReturnType<typeof Bun.spawn>
  private readonly reader: ReadableStreamDefaultReader<Uint8Array>
  private readonly decoder = new TextDecoder()
  private buffer = ""
  private requestId = 0

  constructor(binaryPath: string) {
    this.proc = Bun.spawn([binaryPath], {
      stdin: "pipe",
      stdout: "pipe",
      stderr: "inherit",
    })
    this.reader = (this.proc.stdout as ReadableStream<Uint8Array>).getReader()
  }

  /** Call a bridge method; throws BullCoreError if the sidecar reports !ok. */
  async request<T = unknown>(method: string, args: unknown): Promise<T> {
    const id = String(++this.requestId)
    const payload =
      JSON.stringify({ schema: BRIDGE_REQUEST_SCHEMA, request_id: id, method, args }) + "\n"
    const stdin = this.proc.stdin as { write(s: string): void; flush?(): void }
    stdin.write(payload)
    stdin.flush?.()

    const line = await this.readLine()
    const response = JSON.parse(line) as BullCoreResponse<T>
    if (!response.ok) {
      const error = response.error ?? { code: "unknown", message: "no error detail" }
      throw new BullCoreError(method, error.code, error.message)
    }
    return response.result as T
  }

  private async readLine(): Promise<string> {
    while (!this.buffer.includes("\n")) {
      const { value, done } = await this.reader.read()
      if (done) throw new Error("bull-core sidecar closed unexpectedly")
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
