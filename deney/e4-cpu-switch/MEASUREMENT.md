# E4 — measurement plan (written before any judgment)

## Question

Can a switch learner that runs on a CPU take switches from Jev without
losing calibration, and on which kinds of switch? (`docs/12` §4, §6.)
Three kinds, from E0's capability: **structured** records (ledger, event
log), **free text** within a known set of situations, and **open
vocabulary** (situations never seen in training, known only by their
description).

## Data (truth by construction)

- Situations: E3's 8, plus "no signal". Structured cases rendered from
  E3's templates with new seeds (train 50 per situation, test 25; 10%
  absent). Messages written by the generator model per situation, in
  **separate calls and different styles for train and test** (train
  40 per situation + 40 vague; test 20 + 20), so test messages are not
  near-copies of training ones.
- Jev judges every test case (the baseline) and every training message
  (the teacher's soft labels for distillation).

## Learners (numpy and scipy only)

- **closed**: multinomial logistic regression over hashed words and
  bigrams (2^15), trained on log loss (a proper scoring rule) with L2,
  then one temperature fitted on a validation split (25% of train).
  Trained on truth (`closed-truth`) or on Jev's probabilities
  (`closed-distill`, no truth used).
- **described** (open vocabulary): one binary model over (case, arrow
  description) pairs, features the hashed products of case tokens and
  description tokens plus their overlap; a situation's score is its
  description's probability, normalized over the options. Trained with
  two situations held out entirely, evaluated on those two (4 folds
  covering all 8).

## Measures

- **accuracy** against truth; **top-answer calibration**: ECE over the
  probability of the answer given (10 bins; not one-vs-rest, which E1 and
  E2 showed is swamped by near-zero switches), log loss of the truth.
- **selective prediction**: sort by the learner's probability; the
  largest share of cases it can take while its accuracy on them is at
  least Jev's accuracy on the same test set (**coverage at Jev's
  accuracy**).
- **latency**: wall time per case on this CPU (Apple M4), learner vs
  Jev's recorded latency per call.
- 95% intervals by bootstrap over test cases (2000, seed 0).

## Predictions (directions, with reference values)

- **P1, structured**: the learner reaches Jev's accuracy (within 2
  points) with top-answer ECE no worse than Jev's + 0.02, at under 1 ms
  per case against Jev's ~700 ms.
- **P2, free text, known situations**: lower accuracy than Jev; after
  temperature scaling ECE ≤ 0.05; coverage at Jev's accuracy ≥ 50%.
- **P3, distillation**: `closed-distill` within 5 points of
  `closed-truth` in accuracy, without any truth label.
- **P4, open vocabulary**: on held-out situations the lexical
  `described` learner falls well below its in-vocabulary accuracy
  (below 50%): a lexical CPU model does not meet open vocabulary.

## Rules and threats

- One generation, one Jev pass; failures reported. No test case is
  dropped; test cases never enter training or temperature fitting.
- Threats: generated messages are cleaner than real ones; structured
  channels are templates (P1 may be easy by construction); one teacher;
  hashed lexical features are the simplest possible learner (a lower
  bound on what a CPU learner can do, not its ceiling).
