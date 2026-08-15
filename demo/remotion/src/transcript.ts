import type {TerminalEntry} from "./timeline";

export type Tone = "muted" | "normal" | "cyan" | "amber" | "red" | "violet";

export type StyledTerminalEntry = TerminalEntry & {
  tone?: Tone;
  indent?: number;
};

export const openingTranscript: StyledTerminalEntry[] = [
  {kind: "command", frame: 52, text: "codex -C .", framesPerCharacter: 1.45},
  {kind: "output", frame: 105, text: "╭  OpenAI Codex · FluctlightDB", tone: "violet"},
  {kind: "output", frame: 132, text: "│  model  gpt-5.6-sol     branch  main", tone: "muted"},
  {kind: "output", frame: 172, text: "╰  workspace ready", tone: "cyan"},
  {kind: "prompt", frame: 205, text: "Run two agents in parallel. Share facts, not duplicate strategies.", framesPerCharacter: 0.72, tone: "violet"},
  {kind: "output", frame: 290, text: "codex  Starting a durable swarm run…", tone: "normal"},
  {kind: "output", frame: 320, text: "tool   fluctlight_swarm_begin", tone: "violet"},
  {kind: "output", frame: 346, text: "       roster=[api, tests]  base=44b55b8", tone: "muted"},
  {kind: "output", frame: 372, text: "✓      run swarm-demo-01 · truth revision 7", tone: "cyan"},
];

export const rootSplitTranscript: StyledTerminalEntry[] = [
  {kind: "output", frame: 0, text: "ROOT COORDINATOR", tone: "violet"},
  {kind: "output", frame: 35, text: "swarm-demo-01", tone: "muted"},
  {kind: "output", frame: 82, text: "✓ two slots claimed", tone: "cyan"},
  {kind: "output", frame: 152, text: "alloc overlap", tone: "muted"},
  {kind: "output", frame: 177, text: "0 memory IDs", tone: "cyan", indent: 1},
  {kind: "output", frame: 264, text: "worker-a reports success", tone: "normal"},
  {kind: "output", frame: 304, text: "self-verification blocked", tone: "red"},
  {kind: "output", frame: 348, text: "verifier: cargo test", tone: "amber"},
  {kind: "output", frame: 392, text: "✓ evidence accepted", tone: "cyan"},
  {kind: "output", frame: 434, text: "credit → mem-txn-42 only", tone: "cyan"},
];

export const workerATranscript: StyledTerminalEntry[] = [
  {kind: "output", frame: 0, text: "AGENT api  ·  /worktrees/api", tone: "violet"},
  {kind: "output", frame: 34, text: "✓ slot api claimed", tone: "cyan"},
  {kind: "output", frame: 74, text: "TRUTH    server is sole writer", tone: "cyan"},
  {kind: "output", frame: 106, text: "WARNING  reward() is global", tone: "amber"},
  {kind: "output", frame: 144, text: "MEMORY   mem-txn-42", tone: "normal"},
  {kind: "output", frame: 171, text: "         transaction boundary", tone: "muted"},
  {kind: "output", frame: 226, text: "codex    implementing API path…", tone: "normal"},
  {kind: "output", frame: 330, text: "✓ cargo test · 18 passed", tone: "cyan"},
  {kind: "output", frame: 372, text: "attempt  cited=[mem-txn-42]", tone: "muted"},
];

export const workerBTranscript: StyledTerminalEntry[] = [
  {kind: "output", frame: 15, text: "AGENT tests  ·  /worktrees/tests", tone: "violet"},
  {kind: "output", frame: 48, text: "✓ slot tests claimed", tone: "cyan"},
  {kind: "output", frame: 88, text: "TRUTH    server is sole writer", tone: "cyan"},
  {kind: "output", frame: 120, text: "WARNING  reward() is global", tone: "amber"},
  {kind: "output", frame: 158, text: "MEMORY   mem-crash-17", tone: "normal"},
  {kind: "output", frame: 185, text: "         torn-WAL recovery probe", tone: "muted"},
  {kind: "output", frame: 244, text: "attempt  cite mem-txn-42", tone: "normal"},
  {kind: "output", frame: 276, text: "✕ citation_not_exposed", tone: "red"},
  {kind: "output", frame: 320, text: "codex    switching to assigned memory", tone: "amber"},
  {kind: "output", frame: 388, text: "✓ crash probe passed", tone: "cyan"},
];

export const verificationTranscript: StyledTerminalEntry[] = [
  {kind: "command", frame: 918, text: "python3 scripts/demo_codex_swarm.py", framesPerCharacter: 1.2},
  {kind: "output", frame: 988, text: "starting authenticated coordinator on 127.0.0.1…", tone: "muted"},
  {kind: "output", frame: 1040, text: "✓ PASS  shared truth/warnings + disjoint worker strategies", tone: "cyan"},
  {kind: "output", frame: 1105, text: "✓ PASS  cross-worker memory citation rejected", tone: "cyan"},
  {kind: "output", frame: 1170, text: "✓ PASS  worker self-verification rejected; targeted credit", tone: "cyan"},
  {kind: "output", frame: 1235, text: "↻       stopping coordinator… restarting from WAL", tone: "amber"},
  {kind: "output", frame: 1305, text: "✓ PASS  durable swarm memory survived restart", tone: "cyan"},
  {kind: "command", frame: 1370, text: "git remote get-url origin", framesPerCharacter: 1.05},
  {kind: "output", frame: 1435, text: "https://github.com/voxmastery/FluctlightDB.git", tone: "violet"},
];
