# E0 — the switch ceiling

**Question.** What share of the switches onto's System 1 actually judged
could a CPU switch learner take, as its capability is declared in
`contracts/calibrated-switch.osil`, measured before the learner exists?
(`docs/12-calibrated-switches.md` §6.)

**Lineage.** Branch `deney/e0-switch-ceiling`, from
`research/calibrated-switches` at `d2fda95`.

## Definitions

A **switch** is one judged candidate in a frame record with a judge call:
one-vs-rest for a choice, one per arrow for a noul, one per level for a
score. Its features, per item of the declared capability:

| capability item | measured as |
|---|---|
| refuses `switch.long_context` | the state the model was shown (`seen`, as JSON) is longer than N characters (N = 1000, 2000, 4000) |
| refuses `switch.open_vocabulary` | the frame is open, or a candidate is learned, or the frame is not in the declared category (created by the open world) |
| refuses `switch.unlabelled` | no label source: no **evidence** (the arrow is attested), no **person** (the frame's admission is `assured` or `sealed`), no **resonance** (the frame is in the domain of a functor from its category, or on a path equation) |
| refuses `switch.multimodal` | never: every recorded state is text and JSON |
| admits `switch.binary`, `short_text`, `structured_case`, `labelled_boundary` | the complements; every recorded frame decomposes into binary switches |

A label source here is **available**, not collected: it says labels can
be had (by evidence, a person or a loop), not that they were.

## Data

11 live runs from 2026-09-24 (Jev for judgments), one per scenario, frame
records copied unchanged: `data/manifest.json` names each file's origin
and the category it ran against. 737 switches (718 choice, 12 noul, 7
score); 19 frames belonged to structure the open world had created.

## Run

```sh
cd deney/e0-switch-ceiling
cargo run --release -- --long 1000
cargo run --release -- --long 2000
cargo run --release -- --long 4000
```

Writes `results/ceiling-long{N}.json`.

## Results

| | long > 1000 | long > 2000 | long > 4000 |
|---|---|---|---|
| **ceiling** (admitted share) | 53.9% | **55.2%** | 55.5% |
| refused: long context | 3.1% | 0.4% | 0% |
| refused: open vocabulary | 29.7% | 29.7% | 29.7% |
| refused: unlabelled | 26.7% | 26.7% | 26.7% |

Joint table (N = 2000): what the ceiling becomes if the learner also
admitted…

| also admitted | ceiling |
|---|---|
| nothing | 55.2% |
| open vocabulary | 73.0% |
| unlabelled | 69.9% |
| open vocabulary + unlabelled | **99.6%** |
| all three | 100% |

Per run (N = 2000): support, large taxonomy 100% (208 switches);
infrastructure change 100%; discharge with the patient 100%; consent 76%;
**merger transport 60%**; incident response 38%; hospital discharge 33%;
benefits and catalogue 8%; support alone 0% (repeated gaps, open world);
incident perspectives 0%.

## What it says

1. **Context length is not the constraint.** Median state 297
   characters, 90th percentile 407, largest 2109. A small CPU model's
   input limit is not what binds.
2. **Two refusals bind, and they are complementary.** Open vocabulary
   and missing labels each add about 15–18 points alone (73.0% and
   69.9%), but together 44 points (99.6%). A learner that handles only
   one of them buys little; the build order should come from the joint
   table.
3. **Structure creates labels.** The same Support frames are unlabelled
   on their own (runs 06, 10: 0%) and 60% admitted once the `Merger`
   functor exists (run 11). Adopting a functor made their switches
   checkable by resonance. Resonance is the largest label source (392 of
   737 switches alone, 465 with a person too).
4. **Open frames are the open world's price.** Frames left open for
   learning (the perspectives' entry frames, Support's Account and
   Product, benefits' Circumstances) are exactly those a fixed-vocabulary
   learner refuses.

## Caveats

- Label sources are structural availability, not labels in hand; E1
  measures real labels and Jev's calibration against them.
- "Resonance" counts every frame in a functor's domain or on an equation;
  whether those loops really label cases correctly is E2's question.
- The corpus is small and made of demos (737 switches, 11 runs); support's
  large taxonomy is 28% of it and entirely admitted.
- Runs 02 and 06 are closed-world runs; run 11 used the mock proposer
  for its unrelated gaps (the switches measured are Jev's).

## Next

- **E1**: Jev's calibration per switch where labels exist (the evidence
  and person sources here), with labelled record export.
- A learner design that handles open vocabulary and missing labels
  **together** (for example switches defined per arrow description
  rather than per fixed label set, trained with resonance labels), since
  the joint table says that is where the ceiling moves.
