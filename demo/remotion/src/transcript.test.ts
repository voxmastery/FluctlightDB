import assert from "node:assert/strict";
import {spawnSync} from "node:child_process";
import test from "node:test";

import {openingTranscript, verificationTranscript} from "./transcript.ts";

test("every displayed terminal command uses an installed executable", () => {
  const commands = [...openingTranscript, ...verificationTranscript].filter(
    (entry) => entry.kind === "command",
  );

  for (const command of commands) {
    const executable = command.text.split(" ", 1)[0];
    const lookup = spawnSync("which", [executable], {encoding: "utf8"});
    assert.equal(lookup.status, 0, `displayed executable is unavailable: ${executable}`);
  }
});

test("the displayed Codex working-directory flag is accepted by this Codex CLI", () => {
  const command = openingTranscript.find(
    (entry) => entry.kind === "command" && entry.text.startsWith("codex "),
  );
  assert.ok(command);

  const probe = spawnSync("codex", ["-C", ".", "--version"], {encoding: "utf8"});
  assert.equal(probe.status, 0, probe.stderr);
  assert.equal(command.text, "codex -C .");
});
