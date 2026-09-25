# E3 — measurement plan (written before any judgment)

## 1. What is measured

**Setting.** A support case has a true situation *t* (one of 8, known by
construction). Three perspectives see different channels of it: the
customer's message, the records (ledger), the event log. Each channel
independently shows the truth (clear), nothing unusual (absent), or a
confusable neighbour situation (misleading). Each perspective is a column
of an onto ensemble with its own `state` (it sees only its channel) and
the same judge (Jev).

**Per column i and case c:** the answer *aᵢ(c)*: one of the 8 situations,
or none (the column said `no_signal` or escalated). A column **concluded**
if it answered a situation.

**Per loop S** (any non-empty subset of the three columns, 7 loops):

| measurand | definition |
|---|---|
| **checked** | every column in S concluded (for \|S\| = 1: the column concluded) |
| **U(S), undetected error** | checked, all answers equal, and ≠ *t*; as a share of all cases |
| **D(S), detection** | among checked cases where some answer ≠ *t*: the share where the answers disagree |
| **U_ind(S)** | what U(S) would be if the columns' answers were independent given the truth: Σ over *t* of P(*t*) · Σ over *w* ≠ *t* of Πᵢ P̂ᵢ(*aᵢ* = *w* \| *t*), with P̂ᵢ estimated per column from the same condition's data |
| **R(S) = U(S) / U_ind(S)** | the correlation ratio: 1 when errors are independent, above 1 when they coincide |
| **slope** | least-squares slope of ln U(S) against \|S\| over the 7 loops; under independence and equal error rates it is ≈ ln ē (ē the mean single-column error rate), i.e. U falls by a factor ē per added perspective |
| **k_eff(S)** | ln U(S) / ln ē: the number of effectively independent perspectives in S |

## 2. Checks on the instrument (would the numbers mean anything?)

- **Manipulation check**, per column: when its channel is *clear*, the
  column should answer *t* (accuracy ≥ 0.9); *absent*, answer none (≥ 0.8);
  *misleading*, answer the neighbour (≥ 0.8). Below these, U measures
  the judge's reading errors as much as the designed noise, and is
  reported as such.
- **Negative control**: condition `ind` (independent noise): predicted
  R(S) within [0.5, 2] for every loop with |S| ≥ 2, and slope ≈ ln ē.
- **Positive control**: condition `cor` (the three channels mislead
  together in 35% of cases): predicted R(S) ≫ 1 (above 3) for |S| ≥ 2,
  and U roughly flat in |S| (k_eff near 1). If the metric cannot tell
  `cor` from `ind`, it is not measuring correlation.
- **Pilot**: 16 cases (`pilot`, seed 3) run first, to check the pipeline
  and the manipulation; **excluded** from every result.
- **Messages**: the generator wrote them from an instruction per mode;
  one message per (situation, mode) pool is read by a person-proxy
  (the experimenter) and the reading reported.

## 3. Estimation and uncertainty

- Proportions with Wilson 95% intervals; R, slope and k_eff with 95%
  bootstrap intervals (2000 resamples of cases, within condition, fixed
  seed), U_ind recomputed in each resample.
- Loops whose U is 0 are reported as an upper bound (Wilson), not as 0;
  the slope uses loops with U > 0 and says how many were dropped.

## 4. Power

`ind`, 240 cases, misleading 0.35 per channel: expected U ≈ 0.35 (one
column), 0.12 (two), 0.043 (three), i.e. about 10 undetected errors for
the three-column loop, before the judge's own errors. Realized design:
37% of channels misleading, 11 cases with all three misleading. The
three-column U is therefore estimated from about 10 events: its interval
will be wide (roughly ±60% relative); the slope uses all 7 loops.

## 5. Rules fixed in advance

- One run per condition; a run is repeated only after an API failure,
  and every failure is reported.
- No case is excluded after judging. Cases a column could not judge
  (a model failure) count as that column not concluding.
- Any analysis not listed here is reported as exploratory.

## 6. Threats expected in advance

- **Synthetic truth**: situations and channels are generated; the
  ledger and log are templates, so reading them may be easier than real
  records.
- **One judge**: all columns are Jev; shared biases could correlate
  errors even with independent channels. That is part of what R
  measures, not a flaw to remove.
- **Neighbour structure**: misleading channels point to one fixed
  neighbour per situation, which maximizes coincidence of errors; real
  errors spread over several alternatives, making U smaller.
- **Message noise**: an LLM-written message may not carry its intended
  mode; the manipulation check measures that jointly with Jev's reading.
