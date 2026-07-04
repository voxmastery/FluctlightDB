/**
 * Live smoke test against a running fluctlight-serve instance.
 * Usage: FLUCTLIGHT_SERVE_URL=http://127.0.0.1:8792 npx tsx smoke.test.ts
 */
import FluctlightClient from "./fluctlightdb";

async function main() {
  const client = new FluctlightClient();
  const assert = (cond: unknown, msg: string) => {
    if (!cond) throw new Error(`FAIL: ${msg}`);
    console.log(`ok - ${msg}`);
  };

  assert(await client.health(), "health responds");
  assert(await client.status(), "status responds");

  const exp = (await client.experience("smoke test memory: user prefers dark mode", {
    context: "ts-smoke",
    salience: 0.8,
  })) as Record<string, unknown>;
  assert(exp, "experience writes");

  const act = (await client.activate("dark mode preference")) as Record<string, unknown>;
  assert(act, "activate recalls");
  assert(
    JSON.stringify(act).includes("dark mode"),
    "activate surfaces the written memory",
  );

  const lite = await client.activateLite("dark mode preference");
  assert(lite, "activate-lite responds");

  const batch = await client.activateBatch([{ cue: "dark mode" }, { cue: "smoke test" }]);
  assert(batch, "activate-batch responds");

  const stats = await client.query({ op: "stats" });
  assert(stats, "query stats responds");

  const ticks = await client.tick(1);
  assert(Array.isArray(ticks), "tick returns report array");

  const metrics = await client.metrics();
  assert(typeof metrics === "string" && metrics.length > 0, "metrics returns text");

  console.log("all smoke tests passed");
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
