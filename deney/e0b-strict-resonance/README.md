# E0b — the switch ceiling, with the strict resonance rule

**Question.** E0 counted a frame as having a resonance label source when
it lay in any functor's domain or on a path equation. E2 showed that a
loop of two readings of the same information by the same model labels
nothing: its sides make the same mistake together. What is the ceiling
when only loops over **different information** count?

**Lineage.** Branch `deney/e0b-strict-resonance`, from
`deney/e2-boundary-resonance` at `3eaf651` (the correction comes from
E2). E0 itself is left as measured.

## The rule

A frame has a resonance label source only if its category is a column of
an ensemble (in the module it was run from) in which **another column
declares a different case view** (`state { case: … }`): the sides see
different fields of the case. Evidence (an attested arrow) and a person
(an `assured` or `sealed` frame) count as in E0. Everything else is E0's
code and E0's data, read in place from `../e0-switch-ceiling/data/`.

```sh
cd deney/e0b-strict-resonance
cargo run --release -- --rule loose   # reproduces E0: 55.2%
cargo run --release                   # strict
```

## Results (long context > 2000 characters)

| | E0, loose | **E0b, strict** |
|---|---|---|
| **ceiling** | 55.2% | **14.7%** (108 of 737) |
| refused: open vocabulary | 29.7% | 29.7% |
| refused: unlabelled | 26.7% | **73.4%** |
| ceiling if the learner also handles open vocabulary | 73.0% | **26.3%** |
| … also handles missing labels | 69.9% | **69.9%** |
| … handles both | 99.6% | 99.6% |

Label sources (strict): none 541 · person 81 · resonance 48 · person and
resonance 35 · evidence and person 20 · evidence 12. Resonance now comes
only from the two perspective ensembles (incident: metrics, logs,
complaints; discharge: clinical, social, patient).

| run | switches | loose | strict |
|---|---|---|---|
| support, large taxonomy | 208 | 100% | **0%** |
| merger transport | 152 | 60% | **0%** |
| infrastructure change | 32 | 100% | 100% |
| discharge with the patient | 35 | 100% | 100% |
| consent | 21 | 76% | 76% |
| incident response | 26 | 38% | 38% |
| hospital discharge | 36 | 33% | 33% |
| benefits and catalogue | 40 | 8% | 8% |
| incident perspectives, support alone (3 runs) | 187 | 0% | 0% |

## What it says

1. **E0's ceiling was mostly an artefact.** Two runs (support's large
   taxonomy, the merger) carried 360 of E0's 407 admitted switches, and
   their only loop was a functor over the same text.
2. **Labels bind, not vocabulary.** Under the strict rule, a learner
   that handles missing labels reaches 69.9% alone; one that handles
   open vocabulary reaches 26.3%. E0's joint table suggested they were
   equal partners. They are not: the build order starts with labels.
3. **Where labels come from, today.** Evidence and people's decisions
   (where policy puts a person), and ensembles whose columns see
   different fields. Everything else is judged by Jev with nothing to
   check it against.
4. **So the lever is structural, not a better model.** Designing
   perspectives that see different information (as the incident and
   discharge demos do), and declaring evidence, is what creates labels.
   A functor over the same text does not.

## Caveats

- "Person" still means *available*: at an `assured` or `sealed` frame a
  person decides the gaps, but routine switches there are not reviewed.
  Counting only switches a person actually answered would lower the
  ceiling further; that needs resume (M4).
- Same corpus as E0 (737 switches, 11 demo runs).
