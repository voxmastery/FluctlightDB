#!/usr/bin/env python3
"""Run the Fluctlight Codex swarm lifecycle against a real local server."""

from __future__ import annotations

import json
import os
import signal
import socket
import subprocess
import tempfile
import time
import urllib.error
import urllib.request
import uuid
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
ADMIN_KEY = "demo-admin"
WORKER_KEY = "demo-worker"


def free_port() -> int:
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def transaction(kind: str, **payload: Any) -> dict[str, Any]:
    return {
        "transaction": {
            "kind": kind,
            "payload": {"transaction_id": str(uuid.uuid4()), **payload},
        }
    }


def post(port: int, path: str, body: dict[str, Any], key: str) -> tuple[int, Any]:
    request = urllib.request.Request(
        f"http://127.0.0.1:{port}{path}",
        data=json.dumps(body).encode(),
        headers={
            "Authorization": f"Bearer {key}",
            "Content-Type": "application/json",
        },
        method="POST",
    )
    try:
        with urllib.request.urlopen(request, timeout=5) as response:
            return response.status, json.loads(response.read())
    except urllib.error.HTTPError as error:
        raw = error.read()
        return error.code, json.loads(raw) if raw else {}


def start_server(binary: Path, brain: Path, tenant_root: Path, port: int) -> subprocess.Popen[str]:
    env = os.environ.copy()
    env.update(
        {
            "FLUCTLIGHT_API_KEYS": (
                f"default:{ADMIN_KEY}:admin,default:{WORKER_KEY}:write"
            ),
            "FLUCTLIGHT_STORAGE": "v4",
            "FLUCTLIGHT_WAL_FSYNC": "always",
            "FLUCTLIGHT_TENANT_ROOT": str(tenant_root),
        }
    )
    process = subprocess.Popen(
        [str(binary), "serve", "--addr", f"127.0.0.1:{port}", "--path", str(brain)],
        cwd=ROOT,
        env=env,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE,
        text=True,
    )
    for _ in range(100):
        if process.poll() is not None:
            raise RuntimeError(process.stderr.read() if process.stderr else "server exited")
        try:
            status, _ = post(port, "/api/v1/status", {}, WORKER_KEY)
            if status == 200:
                return process
        except (OSError, urllib.error.URLError):
            pass
        time.sleep(0.05)
    process.terminate()
    raise RuntimeError("server did not become ready")


def stop_server(process: subprocess.Popen[str]) -> None:
    if process.poll() is None:
        process.send_signal(signal.SIGTERM)
        process.wait(timeout=10)


def exposure(memory_id: str, content: str, tag: str) -> dict[str, Any]:
    return {
        "engram_id": memory_id,
        "content": content,
        "score": 0.9,
        "strategy_tags": [tag],
    }


def main() -> None:
    subprocess.run(["cargo", "build", "-q", "-p", "fluctlight-cli"], cwd=ROOT, check=True)
    binary = ROOT / "target" / "debug" / "fluctlight"
    swarm_id = str(uuid.uuid4())
    truth_id, warning_id, actor_id, queue_id = (str(uuid.uuid4()) for _ in range(4))
    port = free_port()

    with tempfile.TemporaryDirectory(prefix="fluctlight-swarm-demo-") as tmp:
        base = Path(tmp)
        brain = base / "brain"
        tenant_root = base / "tenants"
        server = start_server(binary, brain, tenant_root, port)
        try:
            shared_truth = [exposure(truth_id, "Tests are the acceptance authority", "truth")]
            shared_warning = [
                exposure(warning_id, "Do not repeat the stale-snapshot write path", "warning")
            ]
            allocations = {
                "actor-worker": {
                    "verified_truth": shared_truth,
                    "mandatory_warnings": shared_warning,
                    "episodic_memories": [
                        exposure(actor_id, "Explore an actor-owned coordinator", "actor")
                    ],
                    "strict_id_disjoint": True,
                    "diversity_degraded": False,
                },
                "queue-worker": {
                    "verified_truth": shared_truth,
                    "mandatory_warnings": shared_warning,
                    "episodic_memories": [
                        exposure(queue_id, "Explore a transactional queue coordinator", "queue")
                    ],
                    "strict_id_disjoint": True,
                    "diversity_degraded": False,
                },
            }
            roster = [
                {"slot_id": slot, "role": "worker", "agent_id": None, "worktree": None, "status": "declared"}
                for slot in allocations
            ]
            status, _ = post(
                port,
                "/api/v1/swarm/begin",
                transaction(
                    "begin",
                    swarm_id=swarm_id,
                    project_id="codex-hackathon",
                    objective_digest="sha256:parallel-agent-memory",
                    repository_identity="voxmastery/FluctlightDB",
                    base_commit="demo",
                    policy_version="v1",
                    roster=roster,
                    allocations=allocations,
                ),
                ADMIN_KEY,
            )
            assert status == 200

            claimed: dict[str, Any] = {}
            for slot in allocations:
                status, response = post(
                    port,
                    "/api/v1/swarm/claim",
                    transaction(
                        "claim",
                        swarm_id=swarm_id,
                        slot_id=slot,
                        agent_id=f"agent-{slot}",
                        worktree=f"/tmp/{slot}",
                    ),
                    WORKER_KEY,
                )
                assert status == 200
                claimed[slot] = response["value"]

            actor_memories = {m["engram_id"] for m in claimed["actor-worker"]["episodic_memories"]}
            queue_memories = {m["engram_id"] for m in claimed["queue-worker"]["episodic_memories"]}
            assert actor_memories.isdisjoint(queue_memories)
            assert claimed["actor-worker"]["verified_truth"] == claimed["queue-worker"]["verified_truth"]
            assert claimed["actor-worker"]["mandatory_warnings"] == claimed["queue-worker"]["mandatory_warnings"]
            print("PASS: shared truth/warnings, disjoint worker strategies")

            status, _ = post(
                port,
                "/api/v1/swarm/cite",
                transaction("cite", swarm_id=swarm_id, slot_id="actor-worker", memory_ids=[queue_id]),
                WORKER_KEY,
            )
            assert status == 409
            print("PASS: cross-worker memory citation rejected")

            for path, kind, payload in (
                ("/api/v1/swarm/cite", "cite", {"memory_ids": [actor_id]}),
                (
                    "/api/v1/swarm/attempt",
                    "report",
                    {"result_tree": "tree-demo", "summary": "coordinator tests pass"},
                ),
            ):
                status, _ = post(
                    port,
                    path,
                    transaction(kind, swarm_id=swarm_id, slot_id="actor-worker", **payload),
                    WORKER_KEY,
                )
                assert status == 200

            evidence = transaction(
                "evidence",
                swarm_id=swarm_id,
                slot_id="actor-worker",
                receipt={
                    "result": "success",
                    "source_uri": "test://demo",
                    "command_digest": "sha256:demo-test-command",
                },
            )
            worker_status, _ = post(port, "/api/v1/swarm/evidence", evidence, WORKER_KEY)
            assert worker_status == 403
            admin_status, response = post(port, "/api/v1/swarm/evidence", evidence, ADMIN_KEY)
            assert admin_status == 200 and response["value"]["credited_memory_ids"] == [actor_id]
            print("PASS: worker self-verification rejected; cited memory alone received credit")

            status, _ = post(
                port,
                "/api/v1/swarm/finish",
                transaction("finish", swarm_id=swarm_id, accepted_slot_id="actor-worker"),
                ADMIN_KEY,
            )
            assert status == 200
        finally:
            stop_server(server)

        server = start_server(binary, brain, tenant_root, port)
        try:
            status, response = post(
                port, "/api/v1/swarm/get", {"swarm_id": swarm_id}, WORKER_KEY
            )
            assert status == 200 and response["status"] == "finished"
        finally:
            stop_server(server)

    print("PASS: durable, diverse, evidence-gated swarm memory survived restart")


if __name__ == "__main__":
    main()
