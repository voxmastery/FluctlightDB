# benchmarks/test_longmemeval_pref_keys.py
from __future__ import annotations

from longmemeval_bench import expand_queries, user_fact_snippets


GUITAR_Q = (
    "I'm getting excited about my visit to the music store this weekend. "
    "Any tips on what to look for in a new guitar?"
)

GOLD_USER = (
    "I'm considering upgrading from a Fender Stratocaster to a Gibson Les Paul. "
    "Can you tell me the main differences between these two guitars?"
)


def test_guitar_pref_expand_keeps_music_bridge_drops_garden_denver():
    qs = expand_queries(GUITAR_Q, "single-session-preference")
    blob = " ".join(qs).lower()
    assert "guitar" in blob
    assert "music store" in blob or "amplifier" in blob
    assert "homegrown garden" not in blob
    assert "denver colorado" not in blob


def test_movie_pref_expand_prefers_comedy_not_only_docs():
    q = "Can you recommend a show or movie for me to watch tonight?"
    qs = expand_queries(q, "single-session-preference")
    blob = " ".join(qs).lower()
    assert "comedy" in blob or "netflix" in blob
    assert "our planet" not in blob


def test_denver_pref_expand_includes_red_rocks():
    q = "I'm planning a trip to Denver soon. Any suggestions on what to do there?"
    qs = expand_queries(q, "single-session-preference")
    blob = " ".join(qs).lower()
    assert "red rocks" in blob


def test_pref_facts_surface_guitar_brands_from_upgrade():
    facts = user_fact_snippets([GOLD_USER]).lower()
    assert "les paul" in facts or "gibson" in facts
    assert "stratocaster" in facts or "fender" in facts
