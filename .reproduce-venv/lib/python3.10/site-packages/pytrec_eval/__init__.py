"""Module pytrec_eval."""

__version__ = '0.5.10'

import collections
import re
from collections.abc import Callable, Iterable, Mapping
from typing import Dict, List, Set

import numpy as np
from pytrec_eval_ext import RelevanceEvaluator as _RelevanceEvaluator
from pytrec_eval_ext import supported_measures, supported_nicknames

__all__ = [
    'parse_run',
    'parse_qrel',
    'supported_measures',
    'supported_nicknames',
    'RelevanceEvaluator',
]


def parse_run(f_run: Iterable[str]) -> Dict[str, Dict[str, float]]:
    """Parse a TREC run file.

    Each line must be formatted as:
        <query_id> Q0 <doc_id> <rank> <score> <system_name>

    Args:
        f_run: An iterable of run file lines.

    Returns:
        A nested dictionary mapping query IDs to document IDs and scores:
        {
            "q1": {"d1": 2.5, "d2": 1.7, ...},
            "q2": {"d5": 0.8, ...},
        }

    Raises:
        AssertionError: If the same document ID appears twice for the same query.
    """
    run: Dict[str, Dict[str, float]] = collections.defaultdict(dict)
    for line in f_run:
        query_id, _, object_id, ranking, score, _ = line.strip().split()

        assert object_id not in run[query_id]
        run[query_id][object_id] = float(score)

    return run


def parse_qrel(f_qrel: Iterable[str]) -> Dict[str, Dict[str, int]]:
    """Parse a TREC qrel file.

    Each line must be formatted as:
        <query_id> 0 <doc_id> <relevance>

    Args:
        f_qrel: An iterable of qrel file lines.

    Returns:
        A nested dictionary mapping query IDs to document IDs and relevance levels:
        {
            "q1": {"d1": 1, "d2": 0, ...},
            "q2": {"d5": 2, ...},
        }

    Raises:
        AssertionError: If the same document ID appears twice for the same query.
    """
    qrel: Dict[str, Dict[str, int]] = collections.defaultdict(dict)
    for line in f_qrel:
        query_id, _, object_id, relevance = line.strip().split()

        assert object_id not in qrel[query_id]
        qrel[query_id][object_id] = int(relevance)

    return qrel


def compute_aggregated_measure(measure: str, values: List[float]) -> float:
    """Compute an aggregated evaluation measure across queries.

    The aggregation function is determined by the measure name:
        - If the measure starts with "num_", uses sum.
        - If the measure starts with "gm_", uses geometric mean.
        - Otherwise, uses arithmetic mean.

    Args:
        measure: The name of the measure (e.g., "map", "num_ret", "gm_map").
        values: A list of per-query measure values.

    Returns:
        The aggregated score across queries.
    """
    if measure.startswith('num_'):
        agg_fun: Callable[[List[float]], float] = np.sum
    elif measure.startswith('gm_'):

        def agg_fun(values: List[float]) -> float:
            return np.exp(np.sum(values) / len(values))
    else:
        agg_fun: Callable[[List[float]], float] = np.mean  # type: ignore[no-redef]
    return float(agg_fun(values))


class RelevanceEvaluator(_RelevanceEvaluator):
    def __init__(
        self,
        query_relevance: Mapping[str, Mapping[str, int]],
        measures: Iterable[str],
        relevance_level: int = 1,
        judged_docs_only_flag: bool = False,
    ) -> None:
        """Evaluate system runs against TREC-style relevance judgments.

        Args:
            query_relevance: A mapping from query IDs to document IDs and relevance levels.
            measures: A list or set of measures (or nicknames) to compute.
            relevance_level: Minimum relevance level considered relevant (default: 1).
            judged_docs_only_flag: If True, only judged documents are considered.
        """
        measures = self._expand_nicknames(measures)
        measures = self._combine_measures(measures)
        # fixes https://github.com/cvangysel/pytrec_eval/issues/57
        query_relevance = {query_id: qrels for query_id, qrels in query_relevance.items() if len(qrels) > 0}
        super().__init__(
            query_relevance=query_relevance,
            measures=measures,
            relevance_level=relevance_level,
            judged_docs_only_flag=judged_docs_only_flag,
        )

    def evaluate(self, scores: Mapping[str, Mapping[str, float]]) -> Dict[str, Dict[str, float]]:
        """Evaluate a run against the stored qrels.

        Args:
            scores: A mapping from query IDs to document IDs and scores.

        Returns:
            A nested dictionary mapping queries to measures to values:
            {
                "q1": {"map": 0.25, "ndcg": 0.4, ...},
                "q2": {...},
            }
            If `scores` is empty, returns an empty dict.
        """
        if not scores:
            return {}
        return super().evaluate(scores)

    def _expand_nicknames(self, measures: Iterable[str]) -> Set[str]:
        """Expand measure nicknames into their constituent measures."""
        result = set()
        for measure in measures:
            if measure in supported_nicknames:
                result.update(supported_nicknames[measure])
            else:
                result.add(measure)
        return result

    def _combine_measures(self, measures: Iterable[str]) -> Set[str]:
        """Normalize and combine measures into TREC-eval compatible format.

        Handles:
          - Non-parameterized measures: "map"
          - Single-parameter measures: "ndcg.10" or "ndcg_10"
          - Multi-parameter measures: "ndcg.5,10" or "ndcg_5,10"

        Args:
            measures: An iterable of measure strings.

        Returns:
            A set of normalized measure strings in "measure.param1,param2" format.

        Raises:
            ValueError: If an unsupported measure is encountered.
        """
        RE_BASE = r'{}[\._]([0-9]+(\.[0-9]+)?(,[0-9]+(\.[0-9]+)?)*)'

        # break apart measures in any of the following formats and combine
        #  1) meas          -> {meas: {}}  # either non-parameterized measure or use default params
        #  2) meas.p1       -> {meas: {p1}}
        #  3) meas_p1       -> {meas: {p1}}
        #  4) meas.p1,p2,p3 -> {meas: {p1, p2, p3}}
        #  5) meas_p1,p2,p3 -> {meas: {p1, p2, p3}}
        param_meas: dict[str, set[str]] = collections.defaultdict(set)
        for measure in measures:
            if measure not in supported_measures and measure not in supported_nicknames:
                matches = ((m, re.match(RE_BASE.format(re.escape(m)), measure)) for m in supported_measures)
                match = next(filter(lambda x: x[1] is not None, matches), None)
                if match is None:
                    raise ValueError(f'unsupported measure {measure}')
                base_meas, meas_args = match[0], match[1].group(1)  # type: ignore[union-attr]
                param_meas[base_meas].update(meas_args.split(','))
            elif measure not in param_meas:
                param_meas[measure] = set()

        # re-construct in meas.p1,p2,p3 format for trec_eval
        fmt_meas = set()
        for meas, meas_args in param_meas.items():
            if meas_args:
                meas = '{}.{}'.format(meas, ','.join(sorted(meas_args)))
            fmt_meas.add(meas)

        return fmt_meas
