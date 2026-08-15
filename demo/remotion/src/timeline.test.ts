import assert from "node:assert/strict";
import test from "node:test";

import {
  charactersVisible,
  visibleTerminalEntries,
  type TerminalEntry,
} from "./timeline.ts";

test("charactersVisible starts at zero, advances deterministically, and clamps", () => {
  assert.equal(charactersVisible(29, 30, 2, 12), 0);
  assert.equal(charactersVisible(30, 30, 2, 12), 0);
  assert.equal(charactersVisible(36, 30, 2, 12), 3);
  assert.equal(charactersVisible(300, 30, 2, 12), 12);
});

test("visibleTerminalEntries excludes future output and types the active command", () => {
  const entries: TerminalEntry[] = [
    {kind: "output", frame: 10, text: "ready"},
    {kind: "command", frame: 20, text: "codex spawn", framesPerCharacter: 2},
    {kind: "output", frame: 60, text: "worker claimed"},
  ];

  assert.deepEqual(visibleTerminalEntries(entries, 26), [
    {kind: "output", text: "ready", complete: true},
    {kind: "command", text: "cod", complete: false},
  ]);
});
