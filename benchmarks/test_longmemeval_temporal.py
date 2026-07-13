# benchmarks/test_longmemeval_temporal.py
from __future__ import annotations

from longmemeval_bench import session_date_in_window


def test_session_date_in_window_basic():
    assert session_date_in_window("2023-01-15", start="2023-01-01", end="2023-01-31") is True
    assert session_date_in_window("2022-12-01", start="2023-01-01", end="2023-01-31") is False


def test_session_date_in_window_longmemeval_slash_format():
    assert (
        session_date_in_window(
            "2023/05/20 (Sat) 02:21",
            start="2023/05/01",
            end="2023/05/30 (Tue) 23:40",
        )
        is True
    )
    assert (
        session_date_in_window(
            "2023/04/01 (Sat) 00:00",
            start="2023/05/01",
            end="2023/05/30",
        )
        is False
    )


def test_session_date_in_window_unparseable_is_permissive():
    assert session_date_in_window("not-a-date", start="2023-01-01", end="2023-01-31") is True
    assert session_date_in_window("2023-01-15", start=None, end=None) is True
