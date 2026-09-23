# Benefits Assembly

**One coordinated application, assembled from evidence each institution
attests, submitted only with the benefits office's attested approval.**

**Human problem.** A person who loses their job, lives with children and
has a long-term condition faces several programs, each asking for the same
evidence in its own way, at its own pace. Many give up, or are refused
because one document was missing and nobody told them which.

**Beneficiary.** The *cross-system person*: their outcome needs several
institutions to succeed together. `all` makes that one coordinated
obligation; when a line is missing, they are told which one.

## How it works

```
Application (split: every application needs these four lines; no judgment)
  ├─ Income        ─ income_evidenced     attested income.below_threshold (TaxAuthority)  ─┐
  ├─ Residency     ─ residency_evidenced  attested residency.confirmed (CivilRegistry)    ─┤
  ├─ Household     ─ household_evidenced  attested household.confirmed (CivilRegistry)    ─┼─▶ EvidenceComplete  join: all
  └─ Circumstances (choice, judged: a disability or long-term condition?)                  │
        ├─ has_disability ─▶ Disability ─ disability_evidenced (MedicalBoard) ────────────┤
        └─ none_apply ────────────────────────────────────────────────────────────────────┘
EvidenceComplete (split: prepare the application while the office decides)
  ├─ Preparation ─ prepared ──────────────────────────────────────────────┐
  └─ Decision ─ approved  attested eligibility.approved (BenefitsOffice)  ├─▶ Ready  join: gate authority request_decision
                          ensures EligibilityGrant                        ┘                export EligibilityGrant
Ready ─ submit ─▶ Submitted   entry: needs EligibilityGrant
```

```sh
onto run demos/benefits-assembly/benefits.onto --jobs demos/benefits-assembly/jobs --policy shared --max-branches 6
onto ask demos/benefits-assembly/benefits.onto --from Application '<a line from jobs>'
```

## Observed (2026-09-23, live)

| case | evidence | outcome |
|---|---|---|
| B-1 | all four lines incl. disability; office approves | **Submitted**: evidence "completion attested" (TaxAuthority, CivilRegistry ×2, MedicalBoard); "authorized by attested evidence (BenefitsOffice)" |
| B-2 | residency not confirmed | `incomplete_join`: the person is told the residency line is **waiting for evidence** from a trusted source; the other three lines are shown done |
| B-3 | all lines; office attests **not** approved | evidence complete, submission **blocked by the gate** |
| B-4 | no disability described; other lines; approved | **Submitted**, labelled honestly: evidence "completion NOT attested (the circumstances line arrived on judgment alone)" — *no disability* was the judge's reading, not a signed fact — and eligibility authorized on evidence |

## What the judge sees, remembers, and depends on (2026-09-24)

Every case now carries an applicant record: name, birth date,
nationality, ethnicity, gender, postcode. The one judged frame sees only
the person's own statement and precedents:

```
state Circumstances { case: statement; memory: similar 3; }
state { case: statement; }
invariant unseen: case.applicant;
world: closed;
learnable: Circumstances;
```

- **Proved:** no model sees any part of `applicant` (`onto laws`). A
  counterfactual on ethnicity or postcode is therefore unnecessary: the
  judge cannot condition on what it is never shown. Proxies inside the
  statement are the remaining channel (free text).
- **Memory:** `Circumstances` is shown up to three earlier decisions at
  this frame, for *other* cases, each carrying only the statement.
  Precedents are loaded from `benefits.memory.jsonl` (git-ignored) and
  appended after each run.
- **Counterfactual replay:** `onto replay benefits.onto run.jsonl --at
  Circumstances --drop precedents` asks the judge again with the
  recorded state, and without the precedents.

Live, two runs over B-1…B-8 (run 1 with empty memory, run 2 with run 1's
eight precedents):

| case | run 1 (no memory) | run 2 (with precedents) | replay without precedents |
|---|---|---|---|
| B-7 *"I had to stop working to care for my mother, who has dementia"* | `has_disability` (confidence 0.69), **wrong**: the disability is the mother's | **escalated** (0.47): needs a person; the proposer's `provides_care -> Household` refused (Household is sealed) | `has_disability` (0.72) again: **the precedent changed the decision** (baseline replay with precedents: escalate, 0.29) |
| B-5 back pain, B-8 long COVID, MS cases | `has_disability` (1.00) | same | same |
| B-4 no condition, B-6 pregnancy | `none_apply` | same | same |

What this shows, and what it does not:

- The frame has a **MECE gap**: caring for a disabled relative is a
  circumstance the taxonomy does not name, so the judge forced it into
  `has_disability`. That is a policy finding (add a carer circumstance),
  not a model failure.
- **Precedents change decisions.** Here, the contrast with a precedent
  about the person's *own* condition made the judge unsure, which was
  right. The same channel can carry an earlier wrong decision forward:
  run 1's wrong B-7 decision is now a precedent shown to B-5. Memory is
  declared per frame for that reason, and replay measures its effect.
- One baseline per record separates model instability from the effect
  of the change; a single run is evidence, not a statistic.

## What it does not do

It does not submit anything to real institutions or decide eligibility:
the benefits office does, and attests it. It does not know every program
a person could claim; the Circumstances frame is one judged question, and
a missed circumstance is a missed claim (under-claiming is a harm; a
wider noul frame over several circumstances is the next step).
