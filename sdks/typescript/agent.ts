/**
 * FluctlightDB agent brain — TypeScript SDK (HTTP serve or embedded via native wheel).
 */
import { FluctlightClient } from "./fluctlightdb.js";

export type RecallHit = {
  memory_id: string;
  score: number;
  lane: string;
  content: string;
  verified?: boolean;
  snippet?: string;
};

export type RecallResult = {
  mode: string;
  hits: RecallHit[];
  lanes_used: string[];
};

export type AgentBrainOptions = {
  /** Brain directory when using embedded native (Python subprocess bridge). */
  brainPath?: string;
  /** HTTP serve URL when using fluctlight-serve. */
  baseUrl?: string;
  apiKey?: string;
  retainDays?: number;
};

/**
 * Agent-native brain wrapper — mirrors Python `connect_agent()` over HTTP.
 * For embedded native in Node, run `fluctlight-serve` or use the Python SDK.
 */
export class FluctlightAgentBrain {
  private client: FluctlightClient;
  private agentId?: string;

  constructor(opts: AgentBrainOptions = {}) {
    this.client = new FluctlightClient({
      baseUrl: opts.baseUrl,
      apiKey: opts.apiKey,
    });
    this.agentId = opts.brainPath;
  }

  async turnBegin(): Promise<void> {
    await this.client.tick(0);
  }

  async remember(content: string, opts?: { context?: string; salience?: number }): Promise<unknown> {
    return this.client.experience(content, {
      context: opts?.context ?? "turn",
      salience: opts?.salience ?? 0.6,
      agentId: this.agentId,
    });
  }

  async recall(cue: string, opts?: { mode?: string; limit?: number }): Promise<RecallResult> {
    const act = await this.client.activate(cue, { agentId: this.agentId });
    const recalls = (act as { recalls?: Array<Record<string, unknown>> }).recalls ?? [];
    return {
      mode: "episodic",
      hits: recalls.map((r) => ({
        memory_id: String(r.engram_id ?? ""),
        score: Number(r.activation ?? 0),
        lane: "episodic",
        content: String((r.episode as { content?: string })?.content ?? ""),
        verified: Boolean(r.verified),
      })),
      lanes_used: ["episodic"],
    };
  }

  async resolve(cue: string): Promise<Record<string, unknown>> {
    const hits = await this.recall(cue, { limit: 12 });
    const top = hits.hits[0];
    return top
      ? { content: top.content, score: top.score, verified: top.verified ?? false }
      : { content: "", score: 0, verified: false };
  }

  async consolidate(): Promise<unknown> {
    return this.client.consolidate();
  }

  async observeTool(toolName: string, result: string, uri?: string): Promise<unknown> {
    const text = `[${toolName}] ${result}`;
    return this.client.experience(text, {
      context: `tool:${toolName}`,
      salience: 0.72,
      agentId: this.agentId,
    });
  }

  async exportSnapshot(): Promise<unknown> {
    return this.client.query({ op: "stats" });
  }

  status() {
    return this.client.status();
  }
}

export function connectAgent(opts: AgentBrainOptions = {}): FluctlightAgentBrain {
  return new FluctlightAgentBrain(opts);
}

export { FluctlightClient } from "./fluctlightdb.js";
export default FluctlightAgentBrain;
