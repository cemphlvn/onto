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

## What it does not do

It does not submit anything to real institutions or decide eligibility:
the benefits office does, and attests it. It does not know every program
a person could claim; the Circumstances frame is one judged question, and
a missed circumstance is a missed claim (under-claiming is a harm; a
wider noul frame over several circumstances is the next step).
