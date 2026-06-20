#!/usr/bin/env python3
"""Ground agent wallet balance in Fluctlight from wallet.json ledger (verified truth)."""
import json
import os
import re
import sys
import urllib.error
import urllib.request

SERVE = os.environ.get("FLUCTLIGHT_SERVE_URL", "http://127.0.0.1:8792").rstrip("/")
API_KEY = os.environ.get("FLUCTLIGHT_API_KEY", "")
WALLET = os.environ.get(
    "SERVERBRAIN_WALLET",
    os.path.expanduser("~/.sbridge/wallet.json"),
)
REWARDS = os.environ.get(
    "SERVERBRAIN_REWARDS",
    os.path.expanduser("~/.sbridge/rewards.md"),
)


def _headers():
    h = {"Content-Type": "application/json"}
    if API_KEY:
        h["Authorization"] = f"Bearer {API_KEY}"
    return h


def post(path, payload):
    req = urllib.request.Request(
        f"{SERVE}{path}",
        data=json.dumps(payload).encode(),
        headers=_headers(),
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=60) as resp:
        return json.loads(resp.read())


def load_wallet():
    if not os.path.isfile(WALLET):
        return {"balance": 0.0, "level": 1}
    with open(WALLET) as f:
        return json.load(f)


def ground_balance(w):
    bal = float(w.get("balance", 0.0))
    lvl = int(w.get("level", 1))
    content = (
        f"ledger verified: agent wallet balance is ${bal:.2f} at level {lvl} "
        f"(source: wallet.json — ground truth, not chat memory)"
    )
    rep = post(
        "/api/v1/experience",
        {
            "content": content[:500],
            "context": "ledger:wallet",
            "salience": 0.98,
            "doc_id": "ledger",
            "chunk_id": "wallet-balance",
            "source_uri": f"file://{WALLET}",
            "verified": True,
            "provenance_kind": "ledger_verified",
            "confidence": 0.99,
        },
    )
    eid = rep.get("engram_id")
    if eid and not rep.get("deduplicated"):
        post("/api/v1/mark-core", {"engram_id": str(eid), "key": "ledger-wallet-balance"})
    return {"balance": bal, "level": lvl, "report": rep}


def _extract_dollar_amounts(text):
    out = []
    for m in re.finditer(r"\$\s*(\d+(?:\.\d{1,2})?)", text or ""):
        out.append(float(m.group(1)))
    return out


def reconcile_claims(true_balance):
    raw = post("/api/v1/export-raw", {})
    engrams = raw.get("engrams") or []
    conflicts = []
    unverified_chat = 0
    for e in engrams:
        ep = e.get("episode") or {}
        content = ep.get("content") or ""
        low = content.lower()
        if not any(k in low for k in ("balance", "wallet", "$")):
            continue
        rag = ep.get("rag") or {}
        if rag.get("doc_id") == "ledger" and rag.get("chunk_id") == "wallet-balance":
            continue
        prov = ep.get("provenance") or {}
        if prov.get("verified"):
            continue
        amounts = _extract_dollar_amounts(content)
        if amounts and any(abs(a - true_balance) > 0.01 for a in amounts):
            conflicts.append(
                {
                    "engram_id": e.get("id"),
                    "content": content[:120],
                    "claimed": amounts,
                    "true_balance": true_balance,
                }
            )
        elif "assistant" in low or "balance" in low:
            unverified_chat += 1
    return {"conflicts": conflicts[:20], "unverified_chat_balance_mentions": unverified_chat}


def activate_check(true_balance):
    act = post("/api/v1/activate", {"cue": "wallet balance ledger"})
    recalls = act.get("recalls") or []
    top = recalls[0] if recalls else {}
    ep = top.get("episode") or {}
    verified = top.get("verified") or (ep.get("provenance") or {}).get("verified")
    trust = top.get("trust_note")
    return {
        "top_content": (ep.get("content") or "")[:160],
        "top_verified": verified,
        "trust_note": trust,
        "true_balance": true_balance,
    }


def main():
    w = load_wallet()
    grounded = ground_balance(w)
    recon = reconcile_claims(grounded["balance"])
    check = activate_check(grounded["balance"])
    out = {
        "ok": True,
        "wallet_path": WALLET,
        "grounded": grounded,
        "reconcile": recon,
        "recall_check": check,
    }
    print(json.dumps(out, indent=2))
    if recon["conflicts"]:
        print(
            f"\nNote: {len(recon['conflicts'])} chat engrams claim different balances "
            f"(left unverified; ledger engram is ground truth).",
            file=sys.stderr,
        )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except urllib.error.HTTPError as e:
        print(json.dumps({"ok": False, "error": e.read().decode()}), file=sys.stderr)
        raise SystemExit(1)
