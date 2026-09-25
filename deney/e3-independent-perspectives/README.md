# E3 — do loops over different information catch errors?

**Question.** E2 found that a loop of two readings of the *same* text
catches nothing (its sides err together). Does a loop whose perspectives
see *different* information catch errors, and does its undetected error
fall exponentially with the number of perspectives, as independent
perspectives would (a law of the form U ≈ ē^k)?

**Lineage.** Branch `deney/e3-independent-perspectives`, from
`deney/e0b-strict-resonance` at `b9a4a20`. `MEASUREMENT.md` (the plan),
the design, the generator and every case were committed before any
judgment; `NOTES.md` was written during the run.

## Design

- **Truth by construction.** A case is a world state: one of 8 support
  situations (double charge, refund request, failed payment, not
  delivered, damaged, wrong item, account locked, account takeover).
  Three channels are rendered from it: the customer's **message** (drawn
  from pools the generator model wrote per situation and mode), the
  **ledger** (orders, payments, account record) and the **event log**
  (both from templates). Each channel shows the truth (clear), nothing
  unusual (absent) or a confusable neighbour situation (misleading).
- **Three perspectives.** `data/perspectives.onto`: one column per
  channel, each with a `state` that shows only its channel, all judged
  by Jev, compared in a shared `Situation` category (ensemble
  `Perspectives`). One run gives all 7 loops (any subset of the columns).
- **Two conditions.** `ind`, the negative control: channels mislead
  independently (35% each), 240 cases. `cor`, the positive control: the
  three channels mislead together in 35% of cases, 80 cases.

```sh
cd deney/e3-independent-perspectives
python3 generate.py pools && python3 generate.py cases   # already run; output committed
./run.sh pilot && ./run.sh ind && ./run.sh cor              # TYPESAFE_API_KEY
python3 analyze.py ind && python3 analyze.py cor            # results/e3.<condition>.json
```

## Results

**Independent channels: the law holds.**

| loop | k | U (undetected error) [95%] | U if independent | R [95%] | detected when wrong | k_eff |
|---|---|---|---|---|---|---|
| message | 1 | 0.367 | | | | 0.91 |
| ledger | 1 | 0.396 | | | | 0.84 |
| events | 1 | 0.229 | | | | 1.33 |
| message + ledger | 2 | 0.133 [0.096, 0.182] | 0.142 | 0.94 [0.74, 1.15] | 0.76 | 1.82 |
| message + events | 2 | 0.104 [0.072, 0.149] | 0.090 | 1.16 [0.89, 1.43] | 0.73 | 2.04 |
| ledger + events | 2 | 0.088 [0.058, 0.130] | 0.085 | 1.03 [0.76, 1.31] | 0.78 | 2.20 |
| all three | 3 | **0.038** [0.020, 0.070] | 0.033 | 1.14 [0.54, 1.73] | **0.92** | **2.97** |

Slope of ln U on k: **−1.08** [−1.43, −0.87]; the independence
prediction ln ē (ē = 0.33, the mean single-column error) is −1.11.
**Each added perspective divides the undetected error by about 3 (a
factor of ē ≈ 0.34),** and a three-column loop behaves like 2.97
independent perspectives.

**Correlated channels: the law collapses (positive control).**

| loop | k | U | U if independent | R [95%] | detected when wrong | k_eff |
|---|---|---|---|---|---|---|
| two columns (three loops) | 2 | 0.30 to 0.41 | 0.13 to 0.18 | 2.24 to 2.33 [1.61, 2.89] | 0.00 to 0.04 | 0.91 to 1.24 |
| all three | 3 | 0.300 | 0.059 | 5.12 [2.39, 7.34] | 0.04 | 1.24 |

Slope **−0.11** [−0.20, −0.05]: adding perspectives barely helps; three
perspectives are worth about **one**.

The metric told the two conditions apart on every measure: R near 1
against 2 to 5, slope −1.08 against −0.11, k_eff about k against about 1,
detection 0.73 to 0.92 against 0 to 0.04.

## What it says

1. **A law, with its condition.** Undetected error falls as
   **U ≈ ē^k_eff**: exponentially in the number of *effectively
   independent* perspectives. With channels that err independently,
   k_eff ≈ k (here 1.8 to 2.2 for pairs, 2.97 for three). When channels
   err together, k_eff ≈ 1 however many perspectives are added. Same
   information (E2) is the extreme case of the second.
2. **The same judge did not add correlation.** All three columns were
   Jev; with independent channels, R stayed at 1 (every interval
   includes it). What made errors coincide in E2 was the shared text, not
   the shared model.
3. **The price is coverage.** A loop can check a case only when every
   perspective concludes: 91% of cases for message + ledger, 60% for all
   three (the event-log column abstained on a third of cases). Among the
   cases it checked, the three-column loop let 6% of errors through
   (9 of 143), against 40% for one column.
4. **For design (`docs/12`).** Labels from structure are real when the
   perspectives see different information: each independent perspective
   divides undetected error by about 1/ē. This is the quantitative form
   of E0b's lever: *design perspectives over different information*,
   and measure k_eff to know how independent they are.

## Measurement notes (after the experiment)

- **What is established, and what by construction.** The channels were
  generated to err independently, so R ≈ 1 in `ind` partly reflects the
  design. What the data adds is that *reading* them (three columns, one
  judge) introduced no detectable extra correlation, and that the
  measure detects correlation when it is present (`cor`).
- **R is bounded**, by roughly 1/p^(k−1) under perfect correlation, so
  a fixed threshold on it is not meaningful across base rates (a
  pre-registered threshold, R > 3 for pairs, was above that bound:
  `NOTES.md`). k_eff = ln U / ln ē is the comparable scale.
- **U combines coverage and agreement in error**; U_ind models both, so
  R is fair, but the rate among checked cases is reported too.
- **Instrument.** The event-log column read logs less reliably than
  planned (clear 0.82, misleading 0.64), always by abstaining, never by
  a third answer: coverage fell, correlation was not faked or hidden.
- **Precision.** The three-column estimates rest on 9 events (`ind`):
  U [0.020, 0.070], R [0.54, 1.73]. The slope uses all 7 loops.
- **External validity.** The misleading channel always points to one
  fixed neighbour, which maximizes coincidence (real errors spread, so
  real U would be lower); the ledger and log are templates; one judge,
  one generator, synthetic situations.
- **Across E1 to E3, three lessons about the metrics themselves.**
  Calibration averaged over one-vs-rest switches is dominated by trivial
  zeros (E1, E2): use top-answer calibration on boundary cases. A
  generator's label is not truth (E2): generate the world state and
  render the evidence from it (E3's manipulation checks were ≥ 0.95 on
  message and ledger). A ratio bounded by the base rate cannot carry a
  fixed threshold (E3): use k_eff.
