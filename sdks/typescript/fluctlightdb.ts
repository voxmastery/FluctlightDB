/**
 * FluctlightDB TypeScript client — agent episodic memory API.
 */
export class FluctlightClient {
  baseUrl: string;
  apiKey: string;
  timeoutMs: number;

  constructor(opts?: { baseUrl?: string; apiKey?: string; timeoutMs?: number }) {
    this.baseUrl = (opts?.baseUrl ?? process.env.FLUCTLIGHT_SERVE_URL ?? "http://127.0.0.1:8792").replace(/\/$/, "");
    this.apiKey = opts?.apiKey ?? process.env.FLUCTLIGHT_API_KEY ?? "";
    this.timeoutMs = opts?.timeoutMs ?? 60_000;
  }

  private async post<T>(path: string, body: Record<string, unknown> = {}): Promise<T> {
    const headers: Record<string, string> = { "Content-Type": "application/json" };
    if (this.apiKey) headers.Authorization = `Bearer ${this.apiKey}`;
    const res = await fetch(`${this.baseUrl}${path}`, {
      method: "POST",
      headers,
      body: JSON.stringify(body),
      signal: AbortSignal.timeout(this.timeoutMs),
    });
    if (!res.ok) {
      const text = await res.text();
      throw new Error(`Fluctlight HTTP ${res.status}: ${text}`);
    }
    return (await res.json()) as T;
  }

  status() {
    return this.post<Record<string, unknown>>("/api/v1/status");
  }

  experience(content: string, opts?: { context?: string; salience?: number; agentId?: string }) {
    return this.post("/api/v1/experience", {
      content,
      context: opts?.context ?? "api",
      salience: opts?.salience ?? 0.5,
      agent_id: opts?.agentId,
    });
  }

  activate(cue: string, opts?: { agentId?: string; semanticVector?: number[] }) {
    return this.post("/api/v1/activate", {
      cue,
      agent_id: opts?.agentId,
      semantic_vector: opts?.semanticVector,
    });
  }

  ingestChunk(content: string, docId: string, chunkId: string, opts?: { sourceUri?: string; salience?: number }) {
    return this.post("/api/v1/ingest-chunk", {
      content,
      doc_id: docId,
      chunk_id: chunkId,
      source_uri: opts?.sourceUri,
      salience: opts?.salience ?? 0.55,
    });
  }

  consolidate(minSalience = 0.65, limit = 20) {
    return this.post("/api/v1/consolidate", { min_salience: minSalience, limit });
  }

  shardRoute(tenantId: string) {
    return this.post("/api/v1/shard/route", { tenant_id: tenantId });
  }
}

export default FluctlightClient;
