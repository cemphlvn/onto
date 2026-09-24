"""A small, auditable optimizer. No model or task metric is presumed."""

from __future__ import annotations

from dataclasses import asdict, dataclass
from datetime import datetime, timezone
import json
from math import sqrt
from pathlib import Path
from string import Formatter
from typing import Callable, Iterable, Mapping


@dataclass(frozen=True)
class Case:
    id: str
    inputs: Mapping[str, object]
    reference: object


@dataclass(frozen=True)
class Candidate:
    id: str
    text: str
    operation: str
    parent: str | None = None


@dataclass(frozen=True)
class Policy:
    """Declared constraints; learned candidates cannot modify these rules."""

    allowed_operations: frozenset[str]
    variables: frozenset[str]
    required_literals: tuple[str, ...] = ()
    minimum_gain: float = 0.0
    min_cases: int = 2

    def check(self, candidate: Candidate) -> tuple[bool, str]:
        if candidate.operation not in self.allowed_operations:
            return False, "operation is outside declared policy"
        try:
            fields = {name for _, name, _, _ in Formatter().parse(candidate.text) if name}
        except ValueError:
            return False, "invalid template syntax"
        if fields != self.variables:
            return False, f"variables differ: expected {sorted(self.variables)}, got {sorted(fields)}"
        if any("." in field or "[" in field for field in fields):
            return False, "attribute or index access is forbidden"
        if any(literal not in candidate.text for literal in self.required_literals):
            return False, "required literal missing"
        return True, "declared checks passed"


@dataclass(frozen=True)
class Decision:
    candidate_id: str
    admitted: bool
    reason: str
    mean_gain: float | None = None
    lower_bound: float | None = None


@dataclass(frozen=True)
class Result:
    incumbent: Candidate
    decisions: tuple[Decision, ...]


Run = Callable[[str, Case], object]
Score = Callable[[object, Case], float]


class Optimizer:
    def __init__(self, policy: Policy, run: Run, score: Score, log_path: Path | None = None):
        self.policy, self.run, self.score, self.log_path = policy, run, score, log_path

    def _record(self, decision: Decision) -> None:
        if self.log_path is None:
            return
        self.log_path.parent.mkdir(parents=True, exist_ok=True)
        with self.log_path.open("a", encoding="utf-8") as handle:
            handle.write(json.dumps({"time": datetime.now(timezone.utc).isoformat(), **asdict(decision)}) + "\n")

    def _scores(self, candidate: Candidate, cases: tuple[Case, ...]) -> tuple[float, ...]:
        return tuple(self.score(self.run(candidate.text.format(**case.inputs), case), case) for case in cases)

    def optimize(self, baseline: Candidate, candidates: Iterable[Candidate], cases: Iterable[Case]) -> Result:
        cases = tuple(cases)
        if len(cases) < self.policy.min_cases or len({case.id for case in cases}) != len(cases):
            raise ValueError("need enough cases with unique IDs")
        ok, reason = self.policy.check(baseline)
        if not ok:
            raise ValueError(f"baseline fails policy: {reason}")
        incumbent = baseline
        incumbent_scores = self._scores(baseline, cases)
        decisions: list[Decision] = []
        for candidate in candidates:
            ok, reason = self.policy.check(candidate)
            if not ok:
                decision = Decision(candidate.id, False, reason)
            elif candidate.parent != incumbent.id:
                decision = Decision(candidate.id, False, "parent is not current incumbent")
            else:
                scores = self._scores(candidate, cases)
                gains = tuple(new - old for new, old in zip(scores, incumbent_scores))
                mean = sum(gains) / len(gains)
                variance = sum((gain - mean) ** 2 for gain in gains) / (len(gains) - 1)
                lower = mean - 1.96 * sqrt(variance / len(gains))
                admitted = lower > self.policy.minimum_gain
                decision = Decision(candidate.id, admitted,
                                    "measured gain passed gate" if admitted else "gain did not pass gate",
                                    mean, lower)
                if admitted:
                    incumbent, incumbent_scores = candidate, scores
            self._record(decision)
            decisions.append(decision)
        return Result(incumbent, tuple(decisions))
