# E1 — Jev's calibration per switch

**Question.** Where real labels exist, how well calibrated is Jev per
switch: does a probability of p come true about p of the time?
(`docs/12-calibrated-switches.md` §6.)

**Lineage.** Branch `deney/e0-switch-ceiling` (E1 continues E0's lineage),
from `research/calibrated-switches` at `d2fda95`. The labels were
committed **before** the runs (`5121ed1`).

## Labels

`data/labels.json`: one expected arrow per labelled frame, with its basis.

| set | cases | labelled frames | basis |
|---|---|---|---|
| support, large taxonomy (40 intents, flat) | 22 | 22 | `expected` in `large.jobs`: the intent a person chose, written with the demo |
| incident perspectives (metrics, logs, complaints) | 4 | 12 | the job-file comments written before any run, and the case text |
| discharge perspectives (clinical, social, patient) | 5 | 15 | the same; H-3's social label is debatable ("home is fine", after a knee replacement) |

`data/large-flat.onto` is `demos/support-commons/large.onto` without
`grouped by ByTeam`, so every intent is judged in one choice (all 40
switches carry a probability).

## Method

- `run.sh`: each set three times, live Jev (`jev-latest`), closed world
  (nothing learned), mock proposer (no proposal spend), no memory.
- `analyze.py` (standard library, fixed seed): a **switch** is one
  judged candidate (one-vs-rest), its probability the recorded judgment,
  its label 1 for the expected arrow. Expected calibration error (ECE, 10
  equal-width bins), Brier score, log loss; 95% intervals by resampling
  cases. A **decision** is the frame's outcome with Jev's confidence.
  Stability: the spread of each switch's probability over the three
  repeats, and whether the decision was the same.

```sh
cd deney/e1-jev-calibration
./run.sh              # TYPESAFE_API_KEY; uses ../../target/release/onto
python3 analyze.py    # writes results/e1.json
```

## Results

| set | switches | decisions right | escalated | ECE [95%] | Brier | same decision over 3 repeats |
|---|---|---|---|---|---|---|
| support, large taxonomy | 2640 | **66/66** | 0 | 0.000 [0.000, 0.000] | 0.000 | 22/22 (probabilities identical) |
| incident perspectives | 144 | **36/36** | 0 | 0.008 [0.001, 0.017] | 0.001 | 12/12 (spread ≤ 0.03) |
| discharge perspectives | 105 | 42/43 | 2 | 0.073 [0.026, 0.148] | 0.032 | 14/15 (spread ≤ 0.06) |
| **all** | 2889 | 144/145 | 2 | 0.003 [0.001, 0.008] | 0.001 | 48/49 |

Reliability (all switches): 2733 switches at p < 0.1 (none positive),
135 at p ≥ 0.9 (all positive), **21 in between**: 0 from support, 3 from
incident, 18 from discharge.

The one disagreement: **H-3, Social:Home** (after a knee replacement,
"family can drive him and help with shopping"). Over three repeats Jev
said `support_arranged` at confidence 0.62 (taken: the gate is 0.6),
then escalated twice at 0.54 and 0.56. The label (`independent`) was
already marked debatable.

## What it says

1. **On clear cases Jev is decisive, right and stable.** Every labelled
   support ticket, among 40 intents, was right at probability 1.0, with
   identical probabilities in three runs.
2. **These labels cannot measure calibration.** Calibration is a claim
   about the middle of the probability range, and only 21 of 2889
   switches were there. The low ECE is a property of easy cases, not
   evidence of calibration. Measuring it needs a **distribution dense at
   the boundaries** between sibling arrows, which is what the synthetic
   generator of E2 is for, with labels from people.
3. **Where Jev was unsure, the case was genuinely ambiguous.** The only
   frame it disagreed with a label on is the one whose label a person
   had already doubted, and its confidence there (0.54–0.62) sat on the
   gate. A fixed gate of 0.6 turned the same judgment into "follow"
   once and "escalate" twice: an argument for a calibration map and gate
   per switch family (`docs/12` §7, item 3), fitted on labels like these.
4. **For a CPU learner** (E0's "both"): clear switches are easy targets,
   and distilling Jev on them is safe. The value, and the risk, is at
   the boundaries, so the learner must be trained and checked there.

## Caveats

- 31 cases; labels by the demos' author, one debatable.
- Repeats measure Jev's run-to-run stability, not independent samples.
- One model version (`jev-latest`, 2026-09-25).

## Next

- **E2**: synthetic cases at the boundaries between sibling arrows,
  labelled by resonance where a loop can check them and by people where
  not, then E1's measurement repeated on that distribution.
