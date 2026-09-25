# E5c — measurement plan (written before the simulation)

## Question and why

E5 and E5b showed that the self-building loop errs on situations it has
never seen and, more, on known ones; that a good novelty score (AUROC
0.84) is useless without a principled threshold; and that the learner's
calibration was fitted on its most recent labels, so it depended on the
position in the stream. The direction chosen (analysis in the E5b
conversation, recorded in `docs/12`): move the guarantee from the
learner to a **wrapper** that any learner can sit in, so the method can
generalize (E6). E5c identifies which parts of that wrapper matter, and
whether it delivers its guarantee.

**The wrapper, structure-partitioned conformal deferral:** the learner
answers a frame's case only where a risk controller, calibrated on
exchangeable data and keyed by the frame's structure, certifies it; its
abstentions are gaps that reach Jev, and through Jev the open world.

## Stream

E5b's primary stream (660 messages, 10 known situations' worth of
arrows, 2 situations new to the learner arriving in the second half,
Jev's recorded judgments on `message10.onto`), 10 orders. E5's stream
for the primary contrast only.

## Factors (2 × 2 × 2 × 2 = 16 cells)

| factor | level 0 | level 1 |
|---|---|---|
| **C** calibration data | the most recent 40% of labels (position-dependent, as E5) | a random 40% of labels (exchangeable, a reservoir) |
| **G** gate | fixed: top probability ≥ 0.8 | conformal risk control at α = 0.02 |
| **N** novelty | none | the gate's score is min(top probability, novelty rank), where the novelty rank is the case's highest similarity to the training texts, as a percentile of the training texts' own (leave one out) |
| **S** structure | none | the certificate is keyed by the frame's class set: when a label the learner has not trained on enters its labels, its calibration restarts from that point |

Common: the learner (E4's, λ = 1e-4) retrains every 30 cases on the
labels not held out; its temperature is fitted on the held-out ones.
Labels are Jev's answer where Jev answered (p ≥ 0.6), else a person's
(the truth).

**Conformal risk control (G = 1).** Loss of a calibration case at
threshold λ: 1 if the learner would answer (score ≥ λ) and be wrong
against its label. λ is the smallest value with
(n · R̂(λ) + 1) / (n + 1) ≤ α, R̂ the mean loss over the n held-out
labels. With fewer than 49 held-out labels, nothing is certified and the
learner does not answer. The guarantee (under exchangeability):
E[fraction of cases the learner answers wrongly] ≤ α = 0.02.

## The open-world loop (new in E5c)

A new situation's arrow does **not** exist until one of its cases
reaches Jev or a person (a gap: the open world learns the arrow there,
as in `docs/09` run 1). Before that, a new-situation case the learner
answers is wrong by construction. **Discovery latency:** how many cases
of a new situation arrive before its arrow is learned. Jev alone: 0.

## Measures

- **realized risk**: cases the learner answered wrongly (against truth) /
  all cases; per window of 30 cases too.
- **accuracy, resolved per model call, escalations to a person**, as E5.
- **position invariance**: the slope of the learner's error rate (wrong /
  answered) over windows, with a bootstrap interval over orders, and its
  error rate in the 60 cases after the first new-situation case against
  the 60 before.
- **discovery latency** per new situation.
- **attribution**: each factor's main effect on realized risk and on
  resolved per call (the mean over the 8 cells with the factor on minus
  the 8 with it off). Reported, not tested.

## Tests (the only ones)

- **Primary contrast:** the full wrapper (C1 G1 N1 S1) against E5's loop
  (C0 G0 N0 S0), on accuracy and resolved per call.
- **Guarantee:** the full wrapper's realized risk against truth ≤ 0.02
  (mean over orders; the range reported).

## Predictions

- **T1:** the full wrapper's realized risk ≤ 0.02; E5's loop's is higher
  (E5b: about 0.033).
- **T2:** the full wrapper's accuracy ≥ 98% and resolved per call ≥ 1.3×
  (E5's guard reached 98.9% at 1.22× with audits).
- **T3:** position invariance: the full wrapper's error-rate slope over
  windows has an interval containing 0, and its error rate after novelty
  arrives is at most twice the rate before; E5's loop fails both.
- **T4:** discovery latency ≤ 1 case on average for the full wrapper,
  higher for E5's loop.
- **T5 (attribution):** C and G each reduce realized risk; S reduces
  post-novelty errors at a coverage cost.

## Secondary (reported, not tested)

- The full wrapper calibrated on **truth** for every label (an oracle
  teacher) against Jev's answers: realized risk against truth for both
  (the teachers' term of the guarantee).

## Threats

- Exchangeability holds for the reservoir only within a stationary
  stretch; the arrival of new situations breaks it, which is what S
  addresses and what T3 measures.
- α = 0.02 needs 49 held-out labels before anything is certified (about
  123 labels): a guarantee has a start-up cost, reported as coverage.
- Arrows are assumed learned at the first gap (the proposer succeeds);
  Jev's recorded judgments on new situations after discovery come from a
  frame that had the arrows from the start.
- Synthetic messages, one teacher, a correct and free person.
