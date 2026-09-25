# E2 — boundary cases and the team loop

**Question.** Do synthetic cases at the boundaries between sibling
arrows fill the middle of the probability range (where E1 could not
measure calibration), and does a structural loop label those cases
correctly? (`docs/12-calibrated-switches.md` §6.)

**Lineage.** Branch `deney/e2-boundary-resonance`, from
`deney/e1-jev-calibration` at `13d4c3c`. Design, hypotheses, generator
and cases were committed **before** any judgment (the first commit on
this branch).

## Design

- **The loop.** `data/boundary.onto`: the flat 40-intent support
  taxonomy, the 7 teams, `ByTeam` (intent → team), an identity functor,
  and the ensemble `TeamLoop`. Each ticket is judged twice,
  independently: which of 40 intents, which of 7 teams. `ByTeam` says
  which team each intent belongs to, so the two must commute: agreement
  is resonance, a surprise is dissonance.
- **The cases.** `data/design.json`: 12 pairs of sibling intents a
  customer could confuse, 6 within one team and 6 across two;
  `generate.py` asked `~openai/gpt-luna-latest` for tickets at five
  levels (clearly A, leaning A, even, leaning B, clearly B), two each:
  120 cases. Prompts and raw answers are in `data/generation/`. A
  case's label is its generation target; `even` cases have none.
- **Hypotheses** (pre-registered): H1 boundary cases fill the middle of
  the range far more than E1; H2 the loop labels cross-team cases (a
  wrong intent across teams makes it disagree) and is blind within a
  team; H3 calibration becomes measurable and worse than E1.

```sh
cd deney/e2-boundary-resonance
python3 generate.py   # OPENROUTER_API_KEY (already run: its output is committed)
./run.sh              # TYPESAFE_API_KEY; ../../target/release/onto
python3 analyze.py    # results/e2.json
```

## Results

**H1, boundary cases raise uncertainty: supported.**

| | E1 (labelled demo cases) | E2 (boundary cases) |
|---|---|---|
| Jev calls with top probability < 0.9 | 12 of 147 (8%) | **57 of 240 (24%)** |
| mean top probability per call | 0.973 | 0.913 |
| intent switches between p 0.1 and 0.9 | 21 of 2889 | 60 of 4800 (team column: 58 of 840) |

Jev's share for A within the pair falls in order with the level:
1.00 (clearly A) · 0.94 · **0.46 (even)** · 0.09 · 0.00 (clearly B).
Yet on single "even" cases Jev mostly picks a side: 11 of 23 were
decisive (share below 0.1 or above 0.9), 4 split between 0.3 and 0.7.

**H2, the loop as a case label: refuted. As a structure audit: it works.**

- Of 6 intent decisions that differed from the label, the loop flagged
  **none**. Three landed on a third intent in the same team (blind by
  construction). In the other three, both columns read the text the same
  way (the intent and the team agreed with each other, against the
  label).
- Of 9 surprises, 8 had the intent right and the team column
  disagreeing with `ByTeam`, and **5 of those were UI bug reports** ("the
  settings page looks cramped, labels overlap the toggles"): `ByTeam`
  sends them to Product, but the Teams description of Technical reads
  "the product misbehaves". The loop found a contradiction between two
  pieces of policy, not an error in a case.
- Agreement says little about the case: the intent was right in 69 of
  75 agreed cases (92%), against 77 of 83 overall (93%).

**H3, calibration: not established; the labels are the limit.**

Top-answer calibration on labelled cases: 81 decisions at p ≥ 0.9, mean
0.991, 95.1% matching the label; 91 decisions, 93.4% matching. Reading
the six mismatches: two are **label errors** (a "Payment method
declined" screen labelled *error message*, Jev said *payment failed*; a
"reset link expired" labelled *error message*, Jev said *password
reset*), three chose a third intent at least as defensible
(*suspicious login* for a new device in another country), one is mixed.
The generator labelled each case relative to its pair; Jev chose among
all 40. So these labels cannot say whether Jev is overconfident.
(One-vs-rest ECE, 0.0033, is again dominated by 3726 switches near 0.)

## What it says

1. **Boundary cases are the right distribution.** Uncertain calls
   triple, and Jev's probabilities follow the level of ambiguity in
   aggregate. But a model that picks a side on half the genuinely even
   cases is where calibration must be checked, and that needs real labels.
2. **A loop is a label only if its columns see different information.**
   Two readings of the same text by the same model make the same mistake
   together, so their agreement confirms little. The perspectives
   ensembles (metrics, logs, complaints; clinical, social, patient) are
   the kind of loop that can label; this team loop is not. This
   overstates E0: E0 counted every frame in a functor's domain as having
   a resonance label source (392 switches alone). Only loops over
   different information should count.
3. **Loops find contradictions in policy.** The team loop's surprises
   concentrated on one place where the functor and the descriptions
   disagree (UI bugs: Product or Technical). That is a curation signal,
   exactly what ensembles route under `open_world`, and a fix for a
   person: change `ByTeam` or the Teams description.
4. **Synthetic labels must be judged against the whole frame.** A case
   written as "clearly A, not B" can still be a third sibling. A
   generated case needs adjudication against every option (a second
   model, or a person) before its label is used.

## Caveats

- 120 cases, one run, one generator, one judge version; synthetic labels.
- Both columns are Jev, so their errors are expected to correlate; this
  is the finding, not a flaw to correct here.
- 17 cases were incomplete (the team column escalated 12 times, the
  intent column 5), 3 undecided.

## Next

- **E2b**: the same boundary distribution with labels adjudicated
  against all 40 intents (a person, or a second model with the person
  deciding disagreements), then calibration measured on it.
- A resonance label source that counts only loops over **different
  information** (distinct state views or evidence), and E0 recomputed
  with it.
- The UI-bug contradiction (`ByTeam` vs the Teams description) as a
  curation item.
