from __future__ import annotations

import io
import importlib.util
import json
import sys
import unittest
from pathlib import Path
from unittest.mock import patch
from urllib.error import HTTPError

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))


def _load_client_module():
    path = ROOT / "fluctlightdb" / "swarm_client.py"
    spec = importlib.util.spec_from_file_location("fluctlight_swarm_client_test", path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


class _Response:
    def __init__(self, payload: dict):
        self._body = json.dumps(payload).encode("utf-8")

    def __enter__(self):
        return self

    def __exit__(self, *_args):
        return None

    def read(self) -> bytes:
        return self._body


class TestSwarmClient(unittest.TestCase):
    def test_posts_json_with_bearer_and_idempotency_headers(self) -> None:
        module = _load_client_module()
        SwarmClient = module.SwarmClient

        captured = {}

        def fake_urlopen(request, timeout):
            captured["request"] = request
            captured["timeout"] = timeout
            return _Response({"result": "ok"})

        client = SwarmClient("http://127.0.0.1:9471", "worker-token", timeout=3.0)
        with patch.object(module, "urlopen", fake_urlopen):
            result = client.post("/api/v1/swarm/claim", {"transaction": {"kind": "claim"}}, "tx-1")

        self.assertEqual(result, {"result": "ok"})
        self.assertEqual(captured["timeout"], 3.0)
        request = captured["request"]
        self.assertEqual(request.get_header("Authorization"), "Bearer worker-token")
        self.assertEqual(request.get_header("Idempotency-key"), "tx-1")
        self.assertEqual(json.loads(request.data), {"transaction": {"kind": "claim"}})

    def test_raises_structured_error_for_non_success_response(self) -> None:
        module = _load_client_module()
        SwarmClient = module.SwarmClient
        SwarmClientError = module.SwarmClientError

        def fake_urlopen(_request, timeout):
            del timeout
            raise HTTPError(
                "http://127.0.0.1:9471/api/v1/swarm/claim",
                409,
                "Conflict",
                {},
                io.BytesIO(b'{"error":"slot already claimed"}'),
            )

        with patch.object(module, "urlopen", fake_urlopen):
            with self.assertRaises(SwarmClientError) as caught:
                SwarmClient("http://127.0.0.1:9471", "token").post(
                    "/api/v1/swarm/claim", {}, "tx-2"
                )

        self.assertEqual(caught.exception.status, 409)
        self.assertIn("slot already claimed", str(caught.exception))


if __name__ == "__main__":
    unittest.main()
