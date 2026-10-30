# costgate

Calibrate a classifier's confidence in the class it picks, and decide by
cost: act on a prediction when its calibrated probability of being right
reaches `1 - cost_ask / cost_wrong`, ask a person otherwise.

```python
from costgate import CostGatedClassifier
from sklearn.ensemble import RandomForestClassifier

clf = CostGatedClassifier(RandomForestClassifier(), cost_wrong=10, cost_ask=1).fit(X, y)
label, confidence, ask = clf.decide(X_new)   # ask: send these to a person
clf.gate_report(X_test, y_test)              # coverage, accuracy of actions, questions, cost
```

- `method="auto"` (default) picks among identity, Platt, isotonic and
  Venn-Abers by the gate's mean cost on held-out folds, preferring the
  simplest within one standard error. Brier alone is the wrong judge
  here: on onto's architecture-mapping benchmark every calibrator tied
  on Brier while the gate's cost differed by a third
  (`demos/thin-agent/EVALS.md`, Result 6).
- `TopLabelCalibrator` calibrates confidences you already have (an LLM
  judge's p, say) against outcomes.
- Platt here is the twin of onto's `calibrate::Platt` (Rust): the same
  prior towards the identity, the same damped Newton fit.

`uv run --with pytest pytest` runs the tests, scikit-learn's estimator
checks included.
