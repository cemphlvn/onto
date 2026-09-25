# E5b — a novelty signal for the self-building loop

**Question.** Does a novelty signal the learner computes on the CPU,
with a minimum of evidence before acting, bring E5's loop within 1 point
of Jev alone while resolving more intents per model call?

**Lineage.** Branch `deney/e5b-novelty-signal`, from
`deney/e5-self-building-loop` at `15d1884`. Plan and category were
committed before generation; the stream before judgment.

## Design

- **Primary stream (fresh novelty):** E4's 540 messages plus 120 from two
  situations never used before (`subscription_cancel`, `address_change`),
  all judged afresh by Jev on `data/message10.onto`. The first 330 cases
  are known situations; the new ones arrive among the last 330.
  **Secondary:** E5's stream (for comparison only; the detector was
  designed after seeing E5).
- **Policies:** L0 E5's loop · L1 at least 90 labels before acting · L2
  not novel · L3 both · L4 E5's guard (minimum evidence, 10% audits,
  suspension on a new label). Novelty: a case's highest cosine similarity
  to the learner's training texts is below the 5th percentile of the
  training texts' own (leave one out), fixed in advance.

```sh
cd deney/e5b-novelty-signal
../../target/release/onto run data/message10.onto --jobs data/stream.jobs \
  --closed-world --mock-proposer --no-memory --telemetry runs/stream.t.jsonl --dispositions runs/stream.d.jsonl
python3 simulate.py   # results/e5b.json
```

## Results (mean over 10 orders, range)

**Primary stream** (Jev alone: 1.00 per call, accuracy 100%):

| policy | resolved per model call | accuracy | learner's share | learner errors per order, known / new |
|---|---|---|---|---|
| L0 loop | 1.90 (1.68–2.22) | 96.6% | 47% | 18.0 / 4.6 |
| L1 + minimum evidence | 1.88 | 96.5% | 46% | 17.7 / 5.4 |
| L2 + novelty | 1.87 | 96.7% | 46% | 17.5 / 4.4 |
| **L3 both** | **1.84** (1.60–1.96) | **96.9%** (95.9–98.0) | 45% | 16.7 / 4.0 |
| L4 E5's guard | 1.22 (1.03–1.83) | 98.9% | 14% | 6.4 / 0.7 |

**Secondary stream** (E5's; Jev alone 99.8%): L0 1.82× 95.3% (11.3 /
13.2) · L1 1.72× 95.9% (6.9 / 14.3) · L2 1.80× 95.6% · L3 1.70× 96.2%
(7.0 / 12.4) · L4 1.30× 98.3% (4.3 / 3.9).

**Detector:** AUROC **0.842** on the primary stream (127 cases new to the
learner among 6300), 0.776 on the secondary.

Predictions: **R1** (L3 within 1 point of Jev) failed, 96.9% against
100%. **R2** (L3 ≥ 1.5× per call) met, 1.84×. **R3** (AUROC ≥ 0.8) met,
0.842. **R4** (L3 cuts new-situation errors by 80%) failed, by 13%.

## What it says

1. **The score is good; the threshold is not.** The similarity score
   ranks new cases above known ones 84% of the time, but only 2% of the
   cases the learner sees are new to it: at that rate a fixed percentile
   threshold either misses most novelty or blocks many known cases.
   A threshold has to come from a target error rate, not a guess.
2. **Most errors are on known situations**, 16.7 of 20.7 per order on
   the fresh stream, and no novelty signal touches them. Only the audits
   (L4) did, at the cost of coverage.
3. **Minimum evidence depends on the stream**: it cut known-situation
   errors on E5's stream (11.3 → 6.9) but not on the fresh one (18.0 →
   17.7). Its effect is not a property of the policy alone.
4. **A suspected cause, not yet tested:** the learner's temperature is
   fitted on its most recent 25% of labels, so calibration depends on
   where in the stream it happens to be. E5c tests this directly.

## Measurement notes

- **Low base rates defeat good scores.** With 2% prevalence, a
  detector's AUROC says little about its usefulness at any fixed
  threshold; precision at the operating point is what matters.
- **Designed after seeing E5, tested on a fresh stream**: the primary
  stream's new situations were never used before, so R3 is not
  circular; the secondary results are comparison only.
- As in E5: Jev's arrows exist before the open world would learn them
  (the baseline is generous), and a person answers correctly and freely.
