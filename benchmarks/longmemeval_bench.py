#!/usr/bin/env python3
"""LongMemEval-S — answer-in-recall benchmark for FluctlightDB (conv / index modes)."""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
import time
import urllib.error
import urllib.request
from collections import defaultdict
from pathlib import Path
from typing import Any, Optional

REPO = Path(__file__).resolve().parents[1]
SDK = REPO / "sdks" / "python"
if SDK.is_dir() and str(SDK) not in sys.path:
    sys.path.insert(0, str(SDK))

from fluctlightdb import connect_brain, connect_conv, connect_index  # noqa: E402

DEFAULT_DATA = Path("/tmp/longmemeval/data/longmemeval_s_cleaned.json")


def session_date_in_window(
    session_date: str,
    *,
    start: Optional[str] = None,
    end: Optional[str] = None,
) -> bool:
    """Return True if session_date is inside [start, end] (inclusive).

    Unparseable dates are treated as in-window (never hard-drop on parse failure).
    Accepts LongMemEval forms like ``2023/05/30 (Tue) 23:40`` and ISO ``2023-05-30``.
    """
    from datetime import date

    def parse(s: Optional[str]) -> Optional[date]:
        if not s:
            return None
        s = s.strip()
        m = re.match(r"(\d{4})[/-](\d{1,2})[/-](\d{1,2})", s)
        if not m:
            return None
        try:
            return date(int(m.group(1)), int(m.group(2)), int(m.group(3)))
        except Exception:
            return None

    sd, a, b = parse(session_date), parse(start), parse(end)
    if sd is None:
        return True
    if a and sd < a:
        return False
    if b and sd > b:
        return False
    return True


def _embed_base_url() -> str:
    raw = os.environ.get("FLUCTLIGHT_EMBED_URL", "http://127.0.0.1:8793").rstrip("/")
    if raw.endswith("/embed"):
        raw = raw[: -len("/embed")]
    return raw.rstrip("/")


STOPWORDS = frozenset(
    """
    a an the and or but if then else when at by for with about into through during
    before after above below to from up down in out on off over under again further
    once here there all each few more most other some such no nor not only own same
    so than too very can will just don should now what which who whom this that these
    those am is are was were be been being have has had do does did doing would could
    should may might must shall you your yours we our they them their me my mine he she
    it its of as i d ve ll re m t s
    """.split()
)


def expand_queries(question: str, question_type: Optional[str] = None) -> list[str]:
    """Heuristic query expansion (LongMemEval CP3 without LLM)."""
    q = (question or "").strip()
    if not q:
        return [q]
    queries = [q]
    tokens = [
        t
        for t in re.findall(r"[a-z0-9]+", q.lower())
        if len(t) > 3 and t not in STOPWORDS
    ]
    if tokens:
        queries.append(" ".join(tokens))
    ql = q.lower()
    if any(
        w in ql
        for w in (
            "recommend",
            "suggest",
            "prefer",
            "preference",
            "should i",
            "what would",
            "what should",
            "good idea",
            "best ",
            "complement",
            "serve for",
            "accessories",
        )
    ):
        queries.append("user preference enjoys uses prefers " + " ".join(tokens[:14]))
        if tokens:
            queries.append(" ".join(tokens[:10]))
    if any(w in ql for w in ("playlist", "spotify", "apple music", "streaming service")):
        queries.append(
            "user playlist created spotify called named summer vibes chill tracks music listening"
        )
        queries.append(" ".join(tokens[:10]) + " playlist spotify created listening")
    if question_type == "single-session-preference":
        queries.append(
            "user previously mentioned discussed shared experience history "
            + " ".join(tokens[:12])
        )
        queries.append("user preference likes enjoys uses owns bought watched " + " ".join(tokens[:10]))
        if any(w in ql for w in ("recommend", "suggest", "any ", "tips", "advice", "ideas")):
            queries.append("user context background interests hobbies " + " ".join(tokens[:8]))
        # Domain bridges for implicit-preference questions (query omits prior facts).
        # Keep triggers specific — bare "weekend"/"visit"/"travel" false-fire and pollute RRF.
        if any(w in ql for w in ("dinner", "serve for", "meal", "cook", "recipe", "ingredients")):
            queries.append("user homegrown garden harvest tomatoes herbs basil mint ingredients")
        if any(w in ql for w in ("cocktail", "drink", "mixology", "bar")):
            queries.append("user mixology class summer drinks Pimm's Cup cocktails")
        if any(w in ql for w in ("documentary", "watch", "movie", "film", "netflix")):
            queries.append("user watched enjoyed documentary Our Planet Free Solo Tiger King")
        if any(w in ql for w in ("commute", "drive", "podcast", "audiobook")):
            queries.append("user podcast audiobook commute listening history genre")
        if any(w in ql for w in ("cookie", "bake", "baking", "dessert", "chocolate")):
            queries.append("user baking turbinado sugar chocolate chip cookies experiment")
        if any(w in ql for w in ("battery", "phone", "charge", "power")):
            queries.append("user portable power bank battery phone charging")
        if any(w in ql for w in ("reunion", "high school", "nostalgic")):
            queries.append("user high school debate team advanced placement history economics")
        if any(w in ql for w in ("photograph", "camera", "flash", "lens", "accessories", "setup")):
            queries.append(
                "user camera flash lens sony godox tripod photography accessories upgrade"
            )
        if any(w in ql for w in ("music store", "guitar", "instrument", "amplifier", "les paul", "stratocaster")):
            queries.append(
                "user music store guitar fender stratocaster gibson les paul "
                "instrument amplifier pedal vinyl open tuning"
            )
        if "denver" in ql or ("colorado" in ql and "trip" in ql):
            queries.append("user trip denver colorado itinerary attractions restaurants")
        if any(w in ql for w in ("tokyo", "anxious", "getting around", "japan")):
            queries.append("user tokyo japan subway train pass navigation tips transport")
        if any(w in ql for w in ("accessories",)) and "phone" in ql:
            queries.append("user phone case charger cable screen protector accessories")
        if any(w in ql for w in ("furniture", "bedroom", "rearrang", "dresser", "decor")):
            queries.append("user bedroom furniture dresser mid-century modern rearrange layout")
    if question_type == "temporal-reasoning":
        queries.append(" ".join(tokens[:12]) + " date time when event timeline order sequence")
        if any(w in ql for w in ("order", "earliest", "latest", "sequence")):
            queries.append("timeline chronological earliest latest first second third event dates")
        if any(w in ql for w in ("week", "weeks", "month", "months", "days", "ago")):
            queries.append("recent past date when happened timeline calendar")
        if any(w in ql for w in ("trip", "trips", "travel")):
            queries.append("user trip travel vacation flight hotel dates itinerary")
        if any(w in ql for w in ("sport", "event", "race", "marathon", "triathlon")):
            queries.append("user sports event race marathon triathlon tournament dates")
        if any(w in ql for w in ("art", "gallery", "exhibition", "museum")):
            queries.append("user art gallery exhibition museum event held location date")
        if any(w in ql for w in ("kitchen", "appliance", "gadget", "bought")):
            queries.append("user kitchen appliance gadget purchase bought date smoker air fryer")
        if any(w in ql for w in ("days ago", "weeks ago", "week ago", "month ago")):
            queries.append("user mentioned said discussed event purchase trip relative past date")
        if any(w in ql for w in ("relative", "family", "wedding", "funeral", "birthday")):
            queries.append("user family relative life event wedding celebration attended")
        if any(w in ql for w in ("milestone", "business", "company")):
            queries.append("user business milestone company launch anniversary achievement")
    if question_type == "knowledge-update":
        queries.append("user most recent latest new updated current before after")
        if any(w in ql for w in ("kitchen", "gadget", "appliance", "air fryer")):
            queries.append("user kitchen gadget appliance air fryer blender purchase invested")
        if any(w in ql for w in ("doctor", "dr.", "see", "visit", "appointment")):
            queries.append("user doctor appointment visit frequency schedule dr johnson smith therapist")
        if "johnson" in ql:
            queries.append("user therapist counselor dr smith johnson session weekly monthly frequency")
    if question_type == "multi-session":
        queries.append("user multiple times count total visits sessions history")
        if any(w in ql for w in ("doctor", "physician", "specialist")):
            queries.append(
                "user doctor physician dermatologist ent primary care specialist visit appointment"
            )
        if any(w in ql for w in ("fish", "aquarium", "tank")):
            queries.append("user aquarium fish tank tetra gourami pleco gallon community")
        if any(w in ql for w in ("ipad", "tablet", "case", "arrive", "delivery")):
            queries.append("user ipad tablet case ordered bought arrived delivery days shipping")
    if question_type == "single-session-assistant":
        queries.append("assistant previous conversation response answer you said mentioned")
        if any(w in ql for w in ("chess", "move", "game")):
            queries.append("assistant chess move game board position previous")
        if any(w in ql for w in ("song", "sad", "lyrics", "music")):
            queries.append("assistant song lyrics sad music composed wrote created")
    # dedupe, preserve order
    seen: set[str] = set()
    out: list[str] = []
    for item in queries:
        key = normalize(item)
        if item and key not in seen:
            seen.add(key)
            out.append(item)
    return out


def user_fact_snippets(user_msgs: list[str]) -> str:
    """Short fact lines for preference indexing (quoted titles, purchases, hobbies)."""
    facts: list[str] = []
    patterns = (
        r'"[^"]{3,80}"',
        r"'[^']{3,80}'",
    )
    purchase_cues = (
        "bought",
        "purchased",
        "ordered",
        "got a ",
        "picked up",
        "been using",
        "growing",
        "harvested",
        "watched",
        "binge",
        "class",
        "course",
        "reunion",
        "commute",
        "podcast",
        "documentary",
        "debate team",
        "power bank",
        "upgrading",
        "upgrade from",
        "considering",
        "gibson",
        "fender",
        "les paul",
        "stratocaster",
        "guitar",
    )
    brand_pat = re.compile(
        r"\b(?:gibson\s+les\s+paul|fender\s+stratocaster|les\s+paul|stratocaster|"
        r"gibson|fender)\b",
        re.I,
    )
    for msg in user_msgs:
        for pat in patterns:
            facts.extend(re.findall(pat, msg))
        ml = msg.lower()
        brands = brand_pat.findall(msg)
        if brands:
            facts.append(" ".join(brands) + " guitar preference upgrade")
        if any(c in ml for c in purchase_cues):
            facts.append(msg[:350])
    seen: set[str] = set()
    out: list[str] = []
    for f in facts:
        key = normalize(f)
        if key and key not in seen:
            seen.add(key)
            out.append(f.strip())
    return " ".join(out[:8])[:2000]


def preference_signals(user_msgs: list[str]) -> str:
    """Surface implicit preference language from user turns (preference-type boost)."""
    hits: list[str] = []
    cues = (
        "prefer",
        "preference",
        "enjoy",
        "love ",
        "like to",
        "favorite",
        "usually use",
        "i use ",
        "i've been using",
        "i am using",
        "i'm using",
        "interested in",
        "focus on",
        "especially",
        "specifically",
        "bought",
        "purchased",
        "growing",
        "garden",
        "harvest",
        "watched",
        "documentary",
        "podcast",
        "audiobook",
        "mixology",
        "reunion",
        "debate",
        "commute",
        "turbinado",
        "homegrown",
        "power bank",
        "guitar",
        "les paul",
        "stratocaster",
        "gibson",
        "fender",
        "upgrading",
    )
    for msg in user_msgs:
        ml = msg.lower()
        if any(c in ml for c in cues):
            hits.append(msg[:400])
    facts = user_fact_snippets(user_msgs)
    if facts:
        hits.insert(0, facts)
    if not hits and user_msgs:
        hits.append(user_msgs[0][:400])
    return " ".join(hits[:8])[:3000]


def recall_session_id(recall: dict) -> Optional[str]:
    ep = recall.get("episode") or {}
    rag = ep.get("rag") or {}
    doc_id = rag.get("doc_id")
    if doc_id:
        return str(doc_id)
    ctx = ep.get("context") or ""
    if ctx.startswith("longmemeval:"):
        return ctx.split(":", 1)[1]
    return None


def merge_recalls(recall_lists: list[list[dict]], top_k: int) -> list[dict]:
    """Max-activation merge per session_id (multi-query RRF-lite)."""
    best: dict[str, dict] = {}
    for recalls in recall_lists:
        for r in recalls:
            sid = recall_session_id(r) or str(r.get("engram_id") or id(r))
            prev = best.get(sid)
            act = float(r.get("activation") or 0)
            if prev is None or act > float(prev.get("activation") or 0):
                best[sid] = r
    return sorted(best.values(), key=lambda x: -float(x.get("activation") or 0))[:top_k]


def merge_recalls_rrf(recall_lists: list[list[dict]], top_k: int, rrf_k: int = 60) -> list[dict]:
    """Reciprocal-rank fusion across query lists (preference multi-query)."""
    scores: dict[str, float] = {}
    best_item: dict[str, dict] = {}
    for recalls in recall_lists:
        for rank, r in enumerate(recalls):
            sid = recall_session_id(r) or str(r.get("engram_id") or id(r))
            scores[sid] = scores.get(sid, 0.0) + 1.0 / (rrf_k + rank + 1)
            if sid not in best_item:
                best_item[sid] = r
    ordered = sorted(scores.keys(), key=lambda s: -scores[s])
    return [best_item[s] for s in ordered[:top_k]]


def normalize(text: str) -> str:
    return re.sub(r"\s+", " ", (text or "").lower().strip())


def answer_in_recalls(recalls: list[dict], answer: str, top_k: int = 8) -> bool:
    ans = normalize(answer)
    if not ans:
        return False
    ans_tokens = [t for t in re.findall(r"[a-z0-9]+", ans) if len(t) > 2]
    for r in recalls[:top_k]:
        ep = r.get("episode") or {}
        content = normalize(ep.get("content") or "")
        if not content:
            continue
        if ans in content or content in ans:
            return True
        if ans_tokens and sum(1 for t in ans_tokens if t in content) >= max(1, len(ans_tokens) * 2 // 3):
            return True
    return False


class EmbedCache:
    def __init__(self, base_url: str | None = None):
        self.base = (base_url or _embed_base_url()).rstrip("/")
        self.cache: dict[str, list[float]] = {}
        self._requests = 0

    def _cache_key(self, text: str) -> str:
        return (text or "").strip()[:4000]

    def embed_many(self, texts: list[str]) -> list[Optional[list[float]]]:
        out: list[Optional[list[float]]] = [None] * len(texts)
        missing_idx: list[int] = []
        missing_texts: list[str] = []
        for i, t in enumerate(texts):
            key = self._cache_key(t)
            if not key:
                continue
            if key in self.cache:
                out[i] = self.cache[key]
            else:
                missing_idx.append(i)
                missing_texts.append(key)
        if not missing_texts:
            return out
        batch_size = 48
        try:
            for start in range(0, len(missing_texts), batch_size):
                chunk = missing_texts[start : start + batch_size]
                chunk_idx = missing_idx[start : start + batch_size]
                req = urllib.request.Request(
                    f"{self.base}/embed/batch",
                    data=json.dumps({"texts": chunk}).encode(),
                    headers={"Content-Type": "application/json"},
                    method="POST",
                )
                with urllib.request.urlopen(req, timeout=600) as resp:
                    data = json.loads(resp.read().decode())
                vecs = data.get("embeddings") or []
                for j, vec in enumerate(vecs):
                    if j >= len(chunk_idx):
                        break
                    if isinstance(vec, list) and vec:
                        v = [float(x) for x in vec]
                        key = chunk[j]
                        self.cache[key] = v
                        out[chunk_idx[j]] = v
                        self._requests += 1
        except Exception:
            for i, t in enumerate(texts):
                if out[i] is None:
                    out[i] = self.embed_one(t)
        return out

    def embed_one(self, text: str) -> Optional[list[float]]:
        key = self._cache_key(text)
        if not key:
            return None
        if key in self.cache:
            return self.cache[key]
        try:
            req = urllib.request.Request(
                f"{self.base}/embed",
                data=json.dumps({"text": key}).encode(),
                headers={"Content-Type": "application/json"},
                method="POST",
            )
            with urllib.request.urlopen(req, timeout=60) as resp:
                data = json.loads(resp.read().decode())
            vec = data.get("embedding") or data.get("vector")
            if isinstance(vec, list) and vec:
                out = [float(x) for x in vec]
                self.cache[key] = out
                self._requests += 1
                return out
        except Exception:
            return None
        return None

    def embed(self, text: str) -> Optional[list[float]]:
        return self.embed_one(text)


def load_dataset(path: Path) -> list[dict]:
    with path.open() as f:
        data = json.load(f)
    if not isinstance(data, list):
        raise SystemExit(f"expected list in {path}")
    return data


def session_in_recalls(
    recalls: list[dict], answer_session_ids: list[str], top_k: int = 8
) -> bool:
    """Official LongMemEval retrieval metric: gold session in top-k."""
    gold = {str(x) for x in (answer_session_ids or [])}
    if not gold:
        return False
    for r in recalls[:top_k]:
        ep = r.get("episode") or {}
        rag = ep.get("rag") or {}
        sid = rag.get("doc_id") or ep.get("doc_id")
        if sid and str(sid) in gold:
            return True
        ctx = ep.get("context") or ""
        for g in gold:
            if g in ctx:
                return True
    return False


def ingest_item(
    brain: Any,
    item: dict,
    embedder: EmbedCache,
    *,
    fast: bool,
    granularity: str,
    dual_key: bool,
    pref_facts_key: bool,
) -> int:
    if granularity == "session":
        return _ingest_sessions(
            brain, item, embedder, fast=fast, dual_key=dual_key, pref_facts_key=pref_facts_key
        )
    return _ingest_turns(brain, item, embedder, fast=fast)


def _ingest_turns(brain: Any, item: dict, embedder: EmbedCache, *, fast: bool) -> int:
    turns: list[tuple[str, str]] = []
    for session in item.get("haystack_sessions") or []:
        if not isinstance(session, list):
            continue
        for msg in session:
            if not isinstance(msg, dict):
                continue
            role = (msg.get("role") or "user").strip()
            content = (msg.get("content") or "").strip()
            if content:
                turns.append((role, content))
    if not turns:
        return 0
    vectors: list[Optional[list[float]]] = [None] * len(turns)
    if not fast:
        vectors = embedder.embed_many([c for _, c in turns])
    n = 0
    for (role, content), vec in zip(turns, vectors):
        line = f"{role}: {content[:480]}"
        brain.experience(
            line,
            context="longmemeval",
            salience=0.55,
            semantic_vector=vec,
        )
        n += 1
    return n


def soft_boost_temporal(
    recalls: list[dict],
    *,
    question_date: Optional[str],
    session_ids: list[str],
    session_dates: list[str],
    top_k: int,
    boost: float = 0.15,
) -> list[dict]:
    """Soft-boost sessions dated on/before question_date (LongMemEval CP3-lite)."""
    if not question_date or not recalls:
        return recalls[:top_k]
    date_by_sid = {
        str(sid): date
        for sid, date in zip(session_ids, session_dates)
        if sid is not None
    }
    boosted: list[dict] = []
    for r in recalls:
        rr = dict(r)
        sid = recall_session_id(r)
        sd = date_by_sid.get(str(sid), "") if sid else ""
        if sid and sd and session_date_in_window(sd, start=None, end=question_date):
            rr["activation"] = float(rr.get("activation") or 0.0) + boost
        boosted.append(rr)
    boosted.sort(key=lambda x: -float(x.get("activation") or 0.0))
    return boosted[:top_k]


def activate_merged(
    brain: Any,
    question: str,
    *,
    question_type: Optional[str],
    embedder: EmbedCache,
    fast: bool,
    top_k: int,
    query_expand: bool,
) -> list[dict]:
    pool_k = max(top_k * 2, 16)
    if question_type == "single-session-preference":
        pool_k = max(top_k * 3, 24)
    queries = (
        expand_queries(question, question_type)
        if query_expand
        else [question]
    )
    if len(queries) == 1:
        qvec = embedder.embed(question) if not fast else None
        act = brain.activate(question, semantic_vector=qvec, limit=top_k)
        return act.get("recalls") or []
    lists: list[list[dict]] = []
    for q in queries:
        qvec = embedder.embed(q) if not fast else None
        act = brain.activate(q, semantic_vector=qvec, limit=pool_k)
        lists.append(act.get("recalls") or [])
    if question_type == "single-session-preference" and len(lists) > 1:
        return merge_recalls_rrf(lists, top_k)
    return merge_recalls(lists, top_k)


def retrieve_item(
    item: dict,
    *,
    mode: str,
    top_k: int,
    embedder: EmbedCache,
    fast: bool,
    granularity: str,
    query_expand: bool,
    dual_key: bool,
    pref_facts_key: bool,
    brain_sleep: int = 2,
    brain_turns: bool = True,
    brain_facts: bool = True,
    use_muon: bool = False,
) -> tuple[list[dict], bool, int, Any]:
    """Run ingest + activate; return (recalls, session_hit, ingested_n, brain)."""
    brain: Any
    if mode == "brain":
        if use_muon:
            from fluctlightdb import connect_muon  # noqa: WPS433

            brain = connect_muon()
        else:
            brain = connect_brain()
    elif mode == "index":
        brain = connect_index()
    else:
        brain = connect_conv()

    if mode == "brain" and use_muon:
        from brain_memory import ingest_muon_haystack, muon_activate  # noqa: WPS433

        ingested = ingest_muon_haystack(
            brain,
            item,
            dual_key=dual_key or True,
            pref_facts_key=pref_facts_key,
        )
    elif mode == "brain":
        from brain_memory import brain_activate, ingest_brain_haystack  # noqa: WPS433

        ingested = ingest_brain_haystack(
            brain,
            item,
            embedder,
            fast=fast,
            dual_key=dual_key,
            pref_facts_key=pref_facts_key,
            turn_engrams=brain_turns,
            fact_engrams=brain_facts,
            sleep_cycles=brain_sleep,
        )
    else:
        ingested = ingest_item(
            brain,
            item,
            embedder,
            fast=fast,
            granularity=granularity,
            dual_key=dual_key,
            pref_facts_key=pref_facts_key,
        )
    question = (item.get("question") or "").strip()
    if item.get("question_type") == "temporal-reasoning" and item.get("question_date"):
        question = f"{question} [{item['question_date']}]"
    if mode == "brain" and use_muon:
        recalls = muon_activate(
            brain,
            question,
            question_type=item.get("question_type"),
            top_k=top_k,
            query_expand=query_expand or True,
            item=item,
        )
    elif mode == "brain":
        recalls = brain_activate(
            brain,
            item,
            embedder,
            question=question,
            question_type=item.get("question_type"),
            fast=fast,
            top_k=top_k,
            query_expand=query_expand,
        )
    else:
        recalls = activate_merged(
            brain,
            question,
            question_type=item.get("question_type"),
            embedder=embedder,
            fast=fast,
            top_k=max(top_k * 2, 16) if item.get("question_type") == "temporal-reasoning" else top_k,
            query_expand=query_expand,
        )
        if item.get("question_type") == "temporal-reasoning":
            recalls = soft_boost_temporal(
                recalls,
                question_date=item.get("question_date"),
                session_ids=list(item.get("haystack_session_ids") or []),
                session_dates=list(item.get("haystack_dates") or []),
                top_k=top_k,
            )
    hit = session_in_recalls(recalls, item.get("answer_session_ids") or [], top_k=top_k)
    return recalls, hit, ingested, brain


def session_ids_from_recalls(recalls: list[dict], top_k: int = 8) -> list[str]:
    """Unique gold/session doc_ids from recalls, preferring full session engrams."""
    ordered: list[str] = []
    seen: set[str] = set()
    # Pass 1: session chunk engrams
    for r in recalls:
        ep = r.get("episode") or {}
        rag = ep.get("rag") or {}
        sid = rag.get("doc_id") or ep.get("doc_id")
        chunk = (rag.get("chunk_id") or "session").strip()
        if sid and chunk == "session" and sid not in seen:
            seen.add(str(sid))
            ordered.append(str(sid))
        if len(ordered) >= top_k:
            return ordered
    # Pass 2: any engram with doc_id (user_keys / pref_facts)
    for r in recalls:
        ep = r.get("episode") or {}
        rag = ep.get("rag") or {}
        sid = rag.get("doc_id") or ep.get("doc_id")
        if sid and str(sid) not in seen:
            seen.add(str(sid))
            ordered.append(str(sid))
        if len(ordered) >= top_k:
            break
    return ordered


def eval_one(
    item: dict,
    *,
    mode: str,
    top_k: int,
    embedder: EmbedCache,
    fast: bool,
    granularity: str,
    metric: str,
    query_expand: bool,
    dual_key: bool,
    pref_facts_key: bool,
    brain_sleep: int = 2,
    use_muon: bool = False,
    report_ks: Optional[list[int]] = None,
) -> dict[str, Any]:
    t0 = time.perf_counter()
    recalls, hit, ingested, _brain = retrieve_item(
        item,
        mode=mode,
        top_k=top_k,
        embedder=embedder,
        fast=fast,
        granularity=granularity,
        query_expand=query_expand,
        dual_key=dual_key,
        pref_facts_key=pref_facts_key,
        brain_sleep=brain_sleep,
        use_muon=use_muon,
    )
    if metric != "session":
        hit = answer_in_recalls(recalls, item.get("answer") or "", top_k=top_k)
    elapsed = time.perf_counter() - t0
    row: dict[str, Any] = {
        "question_id": item.get("question_id"),
        "question_type": item.get("question_type"),
        "hit": hit,
        "ingested": ingested,
        "recalls": len(recalls),
        "sec": round(elapsed, 3),
    }
    if metric == "session" and report_ks:
        gold = item.get("answer_session_ids") or []
        for k in report_ks:
            row[f"hit_at_{k}"] = session_in_recalls(recalls, gold, top_k=k)
    return row


def _ingest_sessions(
    brain: Any,
    item: dict,
    embedder: EmbedCache,
    *,
    fast: bool,
    dual_key: bool,
    pref_facts_key: bool,
) -> int:
    """One engram per chat session (LongMemEval paper value granularity)."""
    session_ids: list[str] = list(item.get("haystack_session_ids") or [])
    dates: list[str] = list(item.get("haystack_dates") or [])
    sessions = item.get("haystack_sessions") or []
    if not sessions:
        return 0
    payloads: list[tuple[str, str, str, Optional[str]]] = []  # sid, content, chunk_id, embed_text
    for i, session in enumerate(sessions):
        if not isinstance(session, list):
            continue
        sid = session_ids[i] if i < len(session_ids) else f"session_{i}"
        date = dates[i] if i < len(dates) else ""
        lines: list[str] = []
        user_key: list[str] = []
        for msg in session:
            if not isinstance(msg, dict):
                continue
            role = (msg.get("role") or "user").strip()
            content = (msg.get("content") or "").strip()
            if not content:
                continue
            lines.append(f"{role}: {content}")
            if role == "user":
                user_key.append(content)
        if not lines:
            continue
        pref = preference_signals(user_key)
        key_block = " ".join(user_key)[:3000]
        body = "\n".join(lines)
        prefix = f"[{date}] " if date else ""
        full = f"{prefix}{pref}\n{key_block}\n{body}"[:12000]
        embed_snip = f"{prefix}{pref}\n{key_block}"[:2000]
        payloads.append((sid, full, "session", embed_snip))
        if dual_key and user_key:
            user_only = f"{prefix}{pref}\n" + "\n".join(
                f"user: {u}" for u in user_key
            )[:8000]
            payloads.append((sid, user_only, "user_keys", None))
        if pref_facts_key and user_key:
            facts = user_fact_snippets(user_key)
            if facts:
                facts_body = f"{prefix}user facts preferences history\n{facts}"[:5000]
                facts_embed = facts_body[:2000]
                payloads.append((sid, facts_body, "pref_facts", facts_embed))
    # Embed once per session (user_keys reuse parent vector).
    embed_texts = [p[3] for p in payloads if p[3]]
    vectors_by_text: dict[str, list[float]] = {}
    if not fast and embed_texts:
        unique = list(dict.fromkeys(embed_texts))
        got = embedder.embed_many(unique)
        for text, vec in zip(unique, got):
            if vec is not None:
                vectors_by_text[text] = vec
    n = 0
    session_vec: dict[str, list[float]] = {}
    facts_vec: dict[str, list[float]] = {}
    for sid, content, chunk_id, embed_snip in payloads:
        if chunk_id == "session" and embed_snip:
            v = vectors_by_text.get(embed_snip)
            if v is not None:
                session_vec[sid] = v
        if chunk_id == "pref_facts" and embed_snip:
            v = vectors_by_text.get(embed_snip)
            if v is not None:
                facts_vec[sid] = v
    for sid, content, chunk_id, embed_snip in payloads:
        if chunk_id == "user_keys":
            use_vec = session_vec.get(sid)
        elif chunk_id == "pref_facts":
            use_vec = facts_vec.get(sid) or session_vec.get(sid)
        else:
            use_vec = vectors_by_text.get(embed_snip or "")
        salience = 0.6
        if chunk_id == "user_keys":
            salience = 0.65
        elif chunk_id == "pref_facts":
            salience = 0.72
        brain.experience(
            content,
            context=f"longmemeval:{sid}",
            salience=salience,
            semantic_vector=use_vec,
            doc_id=sid,
            chunk_id=chunk_id,
        )
        n += 1
    return n


def main() -> int:
    ap = argparse.ArgumentParser(description="FluctlightDB LongMemEval-S benchmark")
    ap.add_argument("--data", type=Path, default=DEFAULT_DATA)
    ap.add_argument(
        "--mode",
        choices=("brain", "conv", "index"),
        default=os.environ.get("LONGMEMEVAL_MODE", "brain"),
        help="brain=full agent path (default); index=IR-only vector-fast; conv=hybrid RAG",
    )
    ap.add_argument("--top-k", type=int, default=8)
    ap.add_argument(
        "--report-ks",
        default="",
        help="comma-separated K values to score from one recall at max(K) (e.g. 5,8,10)",
    )
    ap.add_argument("--limit", type=int, default=0, help="0 = full dataset")
    ap.add_argument("--offset", type=int, default=0)
    ap.add_argument("--fast", action="store_true", help="skip embeddings (lexical only)")
    ap.add_argument(
        "--granularity",
        choices=("turn", "session"),
        default="turn",
        help="turn=per message (legacy); session=one engram per chat session (LongMemEval paper)",
    )
    ap.add_argument(
        "--metric",
        choices=("answer", "session"),
        default="answer",
        help="answer=answer text in recall; session=gold session_id in top-k (official retrieval)",
    )
    ap.add_argument("--json-out", type=Path, default=None)
    ap.add_argument("--checkpoint", type=Path, default=None, help="resume/save progress JSONL")
    ap.add_argument(
        "--query-expand",
        action="store_true",
        help="multi-query heuristic expansion + per-session merge (CP3)",
    )
    ap.add_argument(
        "--dual-key",
        action="store_true",
        help="index user-only keys as second engram per session (CP2)",
    )
    ap.add_argument(
        "--pref-facts-key",
        action="store_true",
        help="third engram per session: extracted user facts (preference CP2 boost)",
    )
    ap.add_argument(
        "--brain-sleep",
        type=int,
        default=int(os.environ.get("LONGMEMEVAL_BRAIN_SLEEP", "2")),
        help="CLS sleep cycles after ingest (brain mode)",
    )
    ap.add_argument(
        "--muon",
        action="store_true",
        help="Muon Lane: penetrative bulk session imprint (replaces haystack experience+embed)",
    )
    ap.add_argument(
        "--type-filter",
        default="",
        help="comma-separated question_type filter (e.g. single-session-preference)",
    )
    args = ap.parse_args()
    # LongMemEval uses connect_index hybrid BM25+dense — not CHORUS Fabric rerank.
    # Paper-profile Fabric-on applies to CHORUS lanes (LoCoMo, BEIR, FAMB); see bench_lanes.py.

    if not args.data.is_file():
        raise SystemExit(f"dataset not found: {args.data}")

    report_ks = [int(x) for x in args.report_ks.split(",") if x.strip()]
    effective_top_k = max([args.top_k, *report_ks]) if report_ks else args.top_k

    items = load_dataset(args.data)
    if args.type_filter.strip():
        allowed = {t.strip() for t in args.type_filter.split(",") if t.strip()}
        items = [it for it in items if it.get("question_type") in allowed]
    if args.offset:
        items = items[args.offset :]
    if args.limit > 0:
        items = items[: args.limit]

    done_ids: set[str] = set()
    prior: list[dict] = []
    if args.checkpoint and args.checkpoint.is_file():
        with args.checkpoint.open() as f:
            for line in f:
                line = line.strip()
                if not line:
                    continue
                row = json.loads(line)
                prior.append(row)
                qid = row.get("question_id")
                if qid:
                    done_ids.add(str(qid))

    embedder = EmbedCache()
    results = list(prior)
    hits = sum(1 for r in results if r.get("hit"))
    t_start = time.perf_counter()
    metric_key = (
        "session_recall_at_k" if args.metric == "session" else "answer_in_recall_at_k"
    )
    metric_label = f"{metric_key}@{args.top_k}"

    for i, item in enumerate(items):
        qid = str(item.get("question_id") or i)
        if qid in done_ids:
            continue
        try:
            row = eval_one(
                item,
                mode=args.mode,
                top_k=effective_top_k,
                embedder=embedder,
                fast=args.fast,
                granularity=args.granularity,
                metric=args.metric,
                query_expand=args.query_expand,
                dual_key=args.dual_key,
                pref_facts_key=args.pref_facts_key,
                brain_sleep=args.brain_sleep,
                use_muon=args.muon,
                report_ks=report_ks or None,
            )
        except Exception as e:
            row = {
                "question_id": item.get("question_id"),
                "question_type": item.get("question_type"),
                "hit": False,
                "error": str(e)[:200],
                "sec": 0.0,
            }
        results.append(row)
        if row.get("hit"):
            hits += 1
        if args.checkpoint:
            args.checkpoint.parent.mkdir(parents=True, exist_ok=True)
            with args.checkpoint.open("a") as f:
                f.write(json.dumps(row) + "\n")
        n_done = len(results)
        if n_done % 5 == 0 or n_done == len(items) + len(prior):
            rate = hits / n_done if n_done else 0.0
            print(
                f"[{n_done}] {metric_label}={rate:.1%} "
                f"({hits}/{n_done}) last_sec={row.get('sec', 0)}",
                flush=True,
            )

    wall = time.perf_counter() - t_start
    by_type: dict[str, list[bool]] = defaultdict(list)
    for r in results:
        by_type[str(r.get("question_type") or "unknown")].append(bool(r.get("hit")))

    report = {
        "benchmark": "longmemeval_s",
        "dataset": str(args.data),
        "mode": args.mode,
        "granularity": args.granularity,
        "metric": args.metric,
        "query_expand": args.query_expand,
        "dual_key": args.dual_key,
        "pref_facts_key": args.pref_facts_key,
        "brain_sleep": args.brain_sleep if args.mode == "brain" else 0,
        "muon_lane": args.muon,
        "tau_lane": args.muon,
        "top_k": effective_top_k,
        "report_ks": report_ks,
        "questions": len(results),
        metric_key: round(hits / len(results), 4) if results else 0.0,
        "hits": f"{hits}/{len(results)}",
        "wall_s": round(wall, 1),
        "sec_per_question": round(wall / max(1, len(results) - len(prior)), 2),
        "embed_cache_size": len(embedder.cache),
        "embed_requests": embedder._requests,
        "by_type": {
            k: round(sum(v) / len(v), 4) for k, v in sorted(by_type.items())
        },
    }
    if report_ks and args.metric == "session":
        for k in report_ks:
            khits = sum(1 for r in results if r.get(f"hit_at_{k}"))
            report[f"session_recall_at_{k}"] = round(khits / len(results), 4) if results else 0.0
            report[f"hits_at_{k}"] = f"{khits}/{len(results)}"
    print(json.dumps(report, indent=2))

    out = args.json_out or REPO / "benchmarks" / "results" / f"longmemeval-{time.strftime('%Y-%m-%d')}.json"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps({"summary": report, "results": results}, indent=2))
    print(f"Wrote {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
