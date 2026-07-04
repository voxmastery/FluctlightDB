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

  private async request<T>(method: "GET" | "POST", path: string, body?: Record<string, unknown>): Promise<T> {
    const headers: Record<string, string> = {};
    if (body !== undefined) headers["Content-Type"] = "application/json";
    if (this.apiKey) headers.Authorization = `Bearer ${this.apiKey}`;
    const res = await fetch(`${this.baseUrl}${path}`, {
      method,
      headers,
      body: body !== undefined ? JSON.stringify(body) : undefined,
      signal: AbortSignal.timeout(this.timeoutMs),
    });
    if (!res.ok) {
      const text = await res.text();
      throw new Error(`Fluctlight HTTP ${res.status}: ${text}`);
    }
    const contentType = res.headers.get("content-type") ?? "";
    if (!contentType.includes("application/json")) {
      return (await res.text()) as unknown as T;
    }
    return (await res.json()) as T;
  }

  private post<T>(path: string, body: Record<string, unknown> = {}): Promise<T> {
    return this.request<T>("POST", path, body);
  }

  status() {
    return this.post<Record<string, unknown>>("/api/v1/status");
  }

  health() {
    return this.request<Record<string, unknown>>("GET", "/api/v1/health");
  }

  /** Prometheus text-format metrics. */
  metrics() {
    return this.request<string>("GET", "/metrics");
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

  /** Top-1 recall, minimal JSON — pair with HTTP keep-alive for hot paths. */
  activateLite(cue: string, opts?: { agentId?: string; semanticVector?: number[] }) {
    return this.post("/api/v1/activate-lite", {
      cue,
      agent_id: opts?.agentId,
      semantic_vector: opts?.semanticVector,
    });
  }

  /** Batch spreading activation (server caps at 64 cues per call). */
  activateBatch(cues: Array<{ cue: string; agentId?: string; semanticVector?: number[] }>) {
    return this.post("/api/v1/activate-batch", {
      batch: cues.map((c) => ({
        cue: c.cue,
        agent_id: c.agentId,
        semantic_vector: c.semanticVector,
      })),
    });
  }

  /**
   * Brain-native query layer. Ops: list_engrams, list_verified, list_unverified,
   * get_engram, forget, forget_before, search_hybrid, search_by_rag, stats.
   */
  query(query: Record<string, unknown>) {
    return this.post("/api/v1/query", { query });
  }

  /** Run autonomic tick(s) — background maintenance heartbeat. */
  tick(n = 1) {
    return this.post("/api/v1/tick", { n });
  }

  /** Manual sleep cycle — consolidation and pruning. */
  sleep() {
    return this.post("/api/v1/sleep");
  }

  /** Compact the brain store (admin). */
  compact() {
    return this.post("/api/v1/compact");
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
