# E5 — the self-building loop

**Question.** Does a self-building loop (a CPU learner that takes the
cases it is sure of and learns from Jev's answers and people's) resolve
more intents per model call than Jev alone, within the same accuracy
and escalation budget? And how does its share grow with the cases it has
seen? (`docs/12` §6.)

**Lineage.** Branch `deney/e5-self-building-loop`, from
`deney/e4-cpu-switch` at `3e9da06`. `MEASUREMENT.md` was committed before
the simulation; `NOTES.md` during and after.

## Design

- **Stream, no new model calls.** E4's 540 customer messages with Jev's
  recorded judgments and truth by construction. The first 270 cases come
  from 6 situations and vague messages; then 2 new situations
  (`wrong_item`, `account_takeover`) join. 10 stream orders.
- **Jev alone:** every case is a Jev call, followed at p ≥ 0.6, else a
  person answers (correctly).
- **The loop:** E4's learner, retrained every 30 cases on everything
  labelled so far; it answers at p ≥ 0.8; otherwise Jev (its answer
  becomes a label), otherwise a person (the truth becomes a label).

```sh
cd deney/e5-self-building-loop
python3 simulate.py          # pre-registered: results/e5.json
python3 simulate.py guard    # exploratory: results/e5.guard.json
```

## Results (mean over 10 orders, range)

| | Jev alone | the loop (pre-registered) | the loop with a guard (exploratory) |
|---|---|---|---|
| resolved per model call | 1.00 | **1.82** (1.66–1.90) | 1.30 (1.04–1.65) |
| accuracy | **99.8%** | 95.3% (91.3–97.0) | 98.3% (96.1–99.8) |
| escalations to a person | 1.0 | 0.5 | 0.5 |
| learner's share of cases | | 45% | 21% |
| learner errors, known / new situations (all orders) | | 113 / 132 | 43 / 39 |

Growth of the learner's share, pre-registered loop (windows of 30 cases):
0 → 0.08 → 0.40 → 0.42 → 0.62 → 0.59 → 0.65 → 0.71 → **0.74**, then
**collapse to 0.00** when the new situations arrive, and recovery to
0.75 by the end. The first phase fits a saturating exponential better
than a power law (squared error 0.039 against 0.053), but its asymptote
lands on the bound (1.0, τ ≈ 181 cases): saturation is not established
on 9 windows.

New situations: the learner answered 47% of their first 10 cases
(wrong by construction), 22% of the next 10, then 0% while retraining,
and 48% after 70 or more of their cases.

Predictions: **Q1** (≥ 2× per call) missed, 1.82×. **Q2** (within 1
point, no more escalations) missed on accuracy (−4.5 points), met on
escalations. **Q3** (saturating curve near 70–80%) not established: the
exponential fits better, the plateau sits near 0.74, but its fitted
asymptote is unbounded by these data. **Q4** (take-over within a few
dozen cases) missed: about 70, after confident errors.

## What it says

1. **A self-building learner does not know what it does not know.**
   Trained on 6 situations, it answered new ones with confidence, and
   because it answered them, Jev never saw them: the open world could
   not learn the new arrows. This is E4's open-vocabulary failure at
   work in a loop, and it is the dangerous one: silent.
2. **Online, early models are overconfident.** On known situations the
   learner was right on 89.9% of the cases it took, against 99.3% in
   E4 with a full training set: a temperature fitted on a small, recent
   sample is not a calibration.
3. **Efficiency and safety trade off along a curve.** 1.82× per call at
   95.3%, 1.30× at 98.3%; Jev alone 1.00× at 99.8%. A guard made of
   onto's own ideas (minimum evidence, audits, suspension when the
   vocabulary changes) moves along the curve; it does not remove the
   cause.
4. **What the loop needs: a novelty signal the learner can compute.**
   onto knows structurally when a frame's vocabulary changes (a learned
   or declared arrow); the learner must also abstain on inputs unlike its
   training data (a distance to known cases, an open-set class), so that
   novel cases reach Jev, and through Jev the open world. That is E5b.

## Measurement notes (after the experiment)

- **Rates hide mechanisms; decompose errors by source.** The accuracy
  loss had two causes with different fixes (novelty, early
  overconfidence); the headline number showed neither.
- **The baseline was generous**: Jev's recorded judgments already
  include the new situations' arrows, which in onto the open world
  would first learn. Jev alone's accuracy on them is therefore an upper
  bound.
- **The exploratory guard was run once**, chosen from the diagnosed
  mechanisms before running; not tuned on this stream.
- **Growth laws need more points than windows of one stream**: 9
  windows could not separate saturation from slow growth.
- One stream of synthetic messages (E4's), one teacher, a person
  assumed correct and free.
