# Calibration study: which map should the gate use? (pre-registration)

Written and committed **before any calibration model is fitted or any
test record is collected** (2026-10-31). Changes after this commit are
listed at the end with their date and reason; they never touch the
selection rule or the sealed test.

## Question

Which calibration map for `onto discover`'s gate (docs/05 §7) carries
over from the systems it was fitted on to systems it has never seen?

## Unit, switch, label

- **Unit**: one judged decision: a source package, the target module the
  judge chose, its raw p (`onto discover --records`).
- **Switch** (what a map is fitted for): Jev `jev-latest`, architecture
  framing (`evals/sysmdl/sysmdl_eval.py` FRAMING), `--no-roles`, meaning
  only (complete target), module descriptions from the system's own
  documentation. Records from other configurations are not pooled.
- **Label**: the chosen module equals the expert's majority module for
  that package (`.sysmdl` regex mapping, or JabRef's HUSACCT mapping).
  Packages the expert mapping does not cover are dropped.
- Primary analysis unweighted (one decision, one point); class-weighted
  analysis reported as secondary.

## Data and separation

| role | systems | conditions | why |
|---|---|---|---|
| train / cross-validation | JabRef 3.7, TEAMMATES 5.110, ArgoUML r13713 | names visible and hidden (JabRef: visible only, its documentation already names packages) | already seen during this work |
| **sealed test** | Lucene r1075001, Ant r584500, Commons Imaging v1.0a2, Sweet Home 3D r002382, ProM 6.9 (those whose exact sources can be obtained) | visible and hidden | untouched: no run, no blueprint yet |

- **Groups are systems.** Leave-one-system-out over the three training
  systems. A system's visible and hidden records, and any repeats, sit
  in the same fold: they are the same packages.
- **Test blueprints** follow the ArgoUML protocol: written only from each
  system's documentation, package and class names removed, frozen in a
  commit before its first run.
- **Test records are collected but not scored** until the choice below
  is frozen in `choice.json` and committed.

## Candidates (12), from simplest

1. identity (raw p)
2. temperature scaling (a only, b = 0)
3. Platt, prior towards identity, pull 1 (onto's default)
4. Platt, plain (pull 0)
5. Platt on [logit p, logit none]
6. Platt on [logit p, margin p1 − p2]
7. beta calibration (Kull et al. 2017, 3 parameters)
8. histogram binning, 5 equal-width bins
9. histogram binning, 10 equal-mass bins
10. isotonic regression (pool adjacent violators)
11. isotonic, interpolated between steps
12. Venn-Abers (inductive), the mean of its two probabilities

## Selection rule (fixed now)

- **Score**: Brier score (a proper scoring rule) on held-out folds,
  averaged over folds with its standard error.
- **Choice**: the simplest candidate (order above) whose mean Brier is
  within one standard error of the best (one-standard-error rule).
- Reported for all candidates, **not used to choose**: log loss, ECE (5
  bins, L1 gap), reliability diagrams, and at the gate threshold 0.9:
  coverage, precision of adopted decisions, questions asked.

## Then, once

The chosen map is fitted on all training records and scored on the
sealed test, once. Every other candidate's test score is reported
afterwards, labelled post hoc.

## Known weaknesses

- Three training systems give three folds: the standard error is large,
  which is why the one-standard-error rule prefers simple maps.
- ~200 training decisions; the literature expects isotonic to overfit
  below about a thousand (Niculescu-Mizil & Caruana 2005). It is in the
  list so that this is measured, not assumed.
- Gold labels contain disagreements with the documentation (TEAMMATES
  scalability tests, ArgoUML Java reverse engineering): label noise is
  part of what the map learns.
- The gate collects labels where the judge was unsure; maps fitted on
  gate-collected labels later will see a biased sample. This study uses
  full expert labels, not gate-collected ones.

## Changes

(none)
