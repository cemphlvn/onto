# E5 — notes during the experiment

## After the pre-registered run

- Q1 missed (1.82× per call, predicted ≥ 2×) and Q2 missed (accuracy
  95.3% against Jev alone 99.8%, predicted within 1 point).
- Decomposed errors (added to the analysis after the first look, as a
  breakdown of an existing measure, not a new one): over the 10 orders,
  Jev on known situations 8, the learner on known situations 113, the
  learner on the two new situations 132.
- Two mechanisms: **novelty** (the learner, trained on 6 situations,
  answered 47% of a new situation's first 10 cases with confidence,
  necessarily wrong; and since it answered them, Jev never saw them, so
  onto's open world could not learn the new arrow from them); and
  **early overconfidence** (online, the learner trains on few labels and
  a temperature fitted on the last 25% of a small set: its accuracy on
  the cases it took was 89.9%, against 99.3% in E4).
- After the new situations arrive, the learner's share collapses from
  0.66 to 0.00 for about two windows before recovering: retraining on a
  set dominated by cases it gets wrong raises its temperature.

## Exploratory variant (not in the plan)

- One guard, chosen from the two mechanisms before running it, run once:
  no learner answer before 90 labels; 10% of the cases it would take are
  audited by Jev (a model call; Jev's answer used), and the learner acts
  only while it agreed with Jev on ≥ 90% of the last 20 audits; a label
  the learner has never trained on, seen in an audit, suspends it until
  it has 10 examples. Result: accuracy 98.3%, 1.30× per call.
- Not tuned further on this stream, deliberately: trying variants on
  the same data until one looks good is a garden of forking paths. The
  next variant (a novelty detector) is a separate, pre-registered E5b.
