#!/usr/bin/env python3
"""Tests for withdrawn LoCoMo headline detection."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import check_public_locomo_claims as claims


class WithdrawnLocomoValuesTest(unittest.TestCase):
    def test_fraction_unmarked_fails(self) -> None:
        data = {
            "benchmark": "locomo",
            "mean_evidence_recall": 0.99,
            "recall_at_k": {"5": 0.981, "150": 0.9902},
        }
        hits = " ".join(claims.withdrawn_value_paths(data))
        self.assertIn("mean_evidence_recall=0.99", hits)
        self.assertIn("recall_at_k.5=0.981", hits)
        self.assertIn("recall_at_k.150=0.9902", hits)

    def test_percent_unmarked_fails(self) -> None:
        data = {
            "mean_evidence_recall": 99.0,
            "recall_at_k": {"150": 99.0, "20": 98.1},
        }
        hits = " ".join(claims.withdrawn_value_paths(data))
        self.assertIn("mean_evidence_recall=99.0", hits)
        self.assertIn("recall_at_k.150=99.0", hits)
        self.assertIn("recall_at_k.20=98.1", hits)

    def test_fraction_marked_file_passes(self) -> None:
        data = {
            "status": "superseded",
            "retraction": "0.99 LoCoMo evidence recall withdrawn",
            "mean_evidence_recall": 0.99,
            "recall_at_k": {"150": 0.9902, "5": 0.981},
        }
        self.assertEqual(claims.withdrawn_value_paths(data), [])

    def test_percent_marked_object_passes(self) -> None:
        data = {
            "locomo": {
                "status": "retracted",
                "mean_evidence_recall": 98.1,
                "recall_at_k": {"150": 99.0},
            }
        }
        self.assertEqual(claims.withdrawn_value_paths(data), [])

    def test_honest_recall_and_category_cells_pass(self) -> None:
        data = {
            "mean_evidence_recall": 0.968,
            "recall_at_k": {"5": 72.6, "150": 96.8},
            "per_category_at_150": {"temporal": 98.1, "singlehop": 99.0},
            "other": 0.9925,
        }
        self.assertEqual(claims.withdrawn_value_paths(data), [])


if __name__ == "__main__":
    unittest.main()
