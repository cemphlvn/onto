# E3 — notes during the experiment

Written as it ran, in order. The plan is `MEASUREMENT.md`.

## After the pilot (16 cases, excluded from results)

- The pipeline works end to end: 48 frame records, every case answered by
  all three columns.
- Manipulation: Message 16/16 as designed, Ledger 16/16, Events 14/16
  (clear 9/10, misleading 3/4: below the 0.8 threshold on n = 4, to be
  checked on the main run).
- On the measure itself:
  1. R(S) is 1 for single columns by construction (U_ind reproduces a
     column's own error rate); only loops of 2 or 3 carry information
     about correlation. Report R only there.
  2. U(S) is a share of all cases, so it combines coverage (every column
     concluded) with agreement in error. U_ind is computed from the same
     answer distributions, "none" included, so R compares like with like;
     but U should not be read as an error rate among checked cases. Also
     report undetected / checked.
  3. U_ind is estimated within each true situation; with 2 pilot cases per
     situation it is unstable. The main conditions have 30 (`ind`) and 10
     (`cor`) per situation; `cor`'s estimate will be the noisier.

## After the main runs, before interpretation

- No model failures: `ind` 703 followed and 17 escalated of 720 column
  judgments; `cor` 235 and 5 of 240. Escalations count as not concluded.
- **Instrument deviation (Events).** Manipulation below the planned
  thresholds for the event-log column: clear 109/133 (0.82 < 0.9),
  misleading 55/86 (0.64 < 0.8) in `ind`; 0.76 and 0.73 in `cor`. Every
  deviation is an abstention ("none": 76 of 240 in `ind`), never a third
  answer ("other": 0). So the weaker column lowers coverage (fewer loops
  checked) without adding errors that could fake or mask correlation.
  Message and Ledger read as designed (≥ 0.95 everywhere).
- **A pre-registered threshold was unreachable.** The plan predicted
  R > 3 for two-column loops under `cor`. Under perfect correlation a
  pair's R is at most about 1 / p, with p the chance that a column gives
  the coinciding wrong answer (here about 0.41), so about 2.4; observed
  2.24 to 2.33, at that ceiling. R is bounded by the base error rate
  (roughly 1 / p^(k−1)), so it is not comparable across settings or
  loop sizes. k_eff is the scale to compare; R stays a test of
  independence (R = 1), not a strength of correlation.
