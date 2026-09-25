# E4 — notes during the experiment

## Before the learners ran (procedure details the plan left open)

- Regularization chosen from {1e-4, 1e-3, 1e-2} by validation log loss;
  one temperature per learner on the same validation split (25% of the
  training data, seed 7). The distilled learner's temperature is fitted
  against Jev's probabilities, so it never sees a truth label.
- Jev's answer is its highest-probability option (not the gated
  decision), so Jev and the learners are compared on the same footing;
  Jev's escalations are counted separately.
- Leakage check before any judgment: nearest training message by word
  overlap (Jaccard) median 0.30, 95th percentile 0.43, maximum 0.59; no
  duplicates.

## After the run

- Every test and training case was judged by Jev (180 + 200 + 200 test,
  360 training messages); no failures.
- The learner's message calibration missed the prediction (ECE 0.074 >
  0.05), but it is **under**confident (mean top p 0.883, accuracy 0.956).
  Exploratory check: on the validation split, in the training style, ECE
  0.046, accuracy 0.90. The test style turned out easier, so a
  temperature fitted on the harder style left the learner underconfident
  on the easier one. Calibration fitted in one distribution is a claim
  about that distribution; under shift its direction matters, and here
  it erred on the side of deferring.
- Jev on event logs is the reverse: overconfident (mean top p 0.957,
  accuracy 0.90).
