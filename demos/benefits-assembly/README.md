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

## Version 2: the carer circumstance (functor, 2026-09-24)

The first live runs found a MECE gap: a carer was forced into
`has_disability`. Version 2 (`benefits.onto`) adds `is_carer ->
Caring`, evidenced by the civil registry's signed `carer.registered`,
and rewrites the question and the two old options. Version 1 is kept as
`benefits.v1.onto`; `versions.onto` declares `functor Upgrade:
BenefitsAssemblyV1 -> BenefitsAssembly { by name; require: authority,
contracts, invariants; }`.

```
onto functor demos/benefits-assembly/versions.onto
  authority ✓  contracts ✓  invariants ✓           every v1 route exists in v2
  frames whose options changed: Circumstances: gained is_carer
  same structure, changed meaning: has_disability, none_apply, the question at Circumstances
  not covered (new in v2): Caring; is_carer, carer_evidenced

onto migrate demos/benefits-assembly/versions.onto#Upgrade --memory <v1 memory> --to-memory <v2 memory>
  memory: 0 migrated, 15 dropped as stale (their frame's options changed)
```

A precedent is an answer to a question; the question at Circumstances
changed, so v1's answers (including the wrong carer decision) are not
evidence under v2.

Live under v2 (fresh memory):

| case | v1 | v2 (first wording) | v2 (reworded `none_apply`) |
|---|---|---|---|
| B-7 carer | `has_disability` 0.69 (wrong) | `is_carer` 0.99, carer registration **attested** | `is_carer` 1.00 |
| B-4 no condition | `none_apply` 1.00 | **escalated** 0.53 | `none_apply` 0.95 |
| B-6 pregnancy | `none_apply` 0.93 | `none_apply` 0.63 | **escalated** 0.47 |
| B-1/2/3/5/8 | `has_disability` | same | same |

Fixing one gap moved others. Defined by negation ("neither … nor …"),
`none_apply` confused the judge on the plainest case; reworded
positively, it recovered. Pregnancy now escalates: maternity is a
circumstance in many benefit systems, the next gap for a person to
decide, not for the judge to force.

## Transport from a shared catalogue (functor, phase 2, 2026-09-24)

`catalogue.onto` imports this policy and
`standards/support-circumstances.onto` (own condition, caring, maternity,
bereavement, none; `closed`) and declares `functor Catalog:
BenefitsAssembly -> SupportCircumstances { …; transport; }`. `onto
functor` shows the gaps (maternity, bereavement); when a walk escalates
at `Circumstances`, those **empty fibers** are proposed before any LLM.

Live (`onto run demos/benefits-assembly/catalogue.onto#BenefitsAssembly …`):

- **B-6** ("I am pregnant … my employer did not renew my contract"):
  without the catalogue the LLM proposer invented
  `review_pregnancy_related_job_loss -> PregnancyRelatedJobLossReview`.
  With it: *"a related catalogue (Catalog) already knew options this one
  lacked"*: `maternity` and `bereavement` were admitted (the enumeration
  completed from the catalogue), and the judge chose `maternity` at 1.00.
  The next step, the catalogue's `maternity_confirmed`, leads into the
  sealed evidence lines and was refused: the case needs a person to add
  a maternity evidence line, which is a policy decision.
- **A new session**, a widow raising two children: the judge took the
  learned `bereavement` directly (1.00), no expansion at `Circumstances`.
  The option transported for one case served another.

First attempt, and the fix: the critic's overlap check refused both
transported options (*"could one case fit both? yes, p 0.81"* against
`none_apply`). But the catalogue's `Circumstances` frame is **closed**:
people declared its options distinct, and the functor maps `none_apply`
onto the catalogue's `none`. Duplicate and overlap questions between a
transported option and siblings mapping onto *other* options of that
closed frame are now **settled by structure** (recorded as such), not
re-judged by a model. Rule invariants and every structural proof still
apply.

## What it does not do

It does not submit anything to real institutions or decide eligibility:
the benefits office does, and attests it. It does not know every program
a person could claim; the Circumstances frame is one judged question, and
a missed circumstance is a missed claim (under-claiming is a harm; a
wider noul frame over several circumstances is the next step).
