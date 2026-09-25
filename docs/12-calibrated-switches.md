# onto — Calibrated Switches: a Self-Building Intent Graph

Status: **research track, design — 2026-09-25.** Nothing here is built.
It is built on the engine and its bindings (`docs/10-bindings.md`), not
inside them: the bindings stay unopinionated. The realization contract is
`contracts/calibrated-switch.osil` (valid against the OSIL grammar).

## 1. The idea

A graph that builds itself around intents, made of **calibrated binary
switches** that check each other in loops, corrected by people where the
switches are least sure. Each loop is run as a self-contained experiment:
a synthetic distribution, calibration, human feedback, measured before
and after.

**Calibrated decisions** is meant in TypeSafe's sense (RLCD,
reinforcement learning for calibrated decisions): a judge returns
decisions and probabilities, not text, and a probability of 0.8 should
come true about 80% of the time.

## 2. The parts, and where they already are

| part | in onto today | new |
|---|---|---|
| **switch**: one binary judgment, "does this arrow's condition hold for this case?" | a Noul question; every frame reduces to switches (a choice is a mutually exclusive set; `closed` claims they cover every case) | the switch, not the frame, becomes the unit that is measured, calibrated and realized |
| **resonance**: loops of switches that must agree | path equations (two routes agree), functors (images commute), ensembles (columns agree), invariants | agreement around a loop labels a case without a person; disagreement is a gap (a `contradiction` signal, as in ensembles) |
| **synthetic distributions** | the proposer writes cases; an arrow's description is a switch's positive region, its siblings are hard negatives | a generator of synthetic cases per switch, dense at the boundaries between siblings |
| **human feedback** | escalations to a person, held proposals, surprises under `assured`, promotions; attested evidence as verified truth | answers become calibration labels, collected exactly where a switch was unsure (active learning, no extra budget); needs resume with the answer's source (M4) |
| **calibration** | Jev's probabilities, the confidence gate, `onto replay` | calibration measured per switch (reliability curve, expected calibration error, Brier score); a calibration map per switch, fitted on human and evidence labels, applied before the gate |
| **realizations** | Jev (remote) | a second realization of the same switch: onto's own learner, on a CPU (§4) |
| **self-building intent graph** | open world, the library, curation, discovery | an objective: intents resolved per model call, under a calibration-error and escalation budget; it recommends which switches to add, keep dormant, recalibrate or realize locally |

## 3. The loop

```
intent ──▶ walks ──▶ switches judged ──▶ records (state seen, answer, outcome)
   ▲                                          │
   │                  labels ◀────────────────┤  person (escalations, resume)
   │                                          │  evidence (attested observations)
   │                                          │  resonance (loops that agree)
   │                                          ▼
   │                          calibration per switch (measure, map)
   │                                          │
   │           synthetic cases at the boundaries (proposer, per switch)
   │                                          │
   └──── structure and realizations: add · recall · dormant · local switch
         (through the admission regimes: recommendations first, D64)
```

The loop never bypasses policy. Structure changes go through the same
admission regimes and proofs as today. Its metrics recommend before they
act: the call-economy rule (`docs/08` §5) holds here too.

## 4. Realizations of a switch

1. **Jev (remote).** If TypeSafe accepts switch-level training data
   (records exported with human, evidence and resonance labels), onto
   feeds its training directly. Jev keeps its own meaning: onto does not
   redefine it (OSIL's sovereignty principle).
2. **onto's own learner (CPU).** If not, onto improves its own training
   method, one that runs on a CPU. The starting candidates are measured
   against each other, not assumed:
   - a calibration map on Jev's outputs (no training: isotonic or
     temperature per switch);
   - a distilled switch: Jev's probabilities as soft labels, people and
     evidence as hard labels, a small model (for example a frozen small
     text encoder with a logistic head, or boosted trees over structured
     case fields), trained on a proper scoring rule so that calibration
     is the objective;
   - resonance as a self-supervised signal on synthetic cases.

   Its contract is declared before it is built:
   `contracts/calibrated-switch.osil` names the concept
   (`CalibratedSwitch`, realized `to { jev_remote, cpu_switch_local }`),
   what a replacement must preserve (decision semantics, calibration,
   abstention, the state policy and its unseen fields, provenance), the
   CPU model's constraints (latency, memory, calibration error), and a
   **capability**: which switch kinds the learner admits or refuses.

## 5. Each loop as an experiment

Every loop is written up the same way:

- **hypothesis** and the switch family (a demo's frames);
- **the distribution**: recorded cases, synthetic cases, and how they
  were generated;
- **labels**: how many from people, evidence and resonance, and at what
  cost;
- **calibration before and after**: reliability curves, expected
  calibration error, Brier score, per switch;
- **effect on the system**: escalations, model calls, wall time,
  decisions changed (replay);
- the artifacts (records, labels, maps, models) to reproduce it.

## 6. Experiments, in order

| # | question | data | needs |
|---|---|---|---|
| E0 ✓ | **ceiling**: what share of recorded switches the CPU capability admits, before building anything (OSIL's capability-ceiling method) | frame records of every demo run | a switch classifier over records. **Result** (`deney/e0-switch-ceiling`): 55.2% of 737 switches; context length does not bind; open vocabulary and missing labels bind together (99.6% if both are handled, 73.0% and 69.9% alone); adopting a functor made 60% of Support's switches labelable by resonance. **Corrected by E0b** (`deney/e0b-strict-resonance`): counting only loops over different information (E2), the ceiling is **14.7%**; missing labels bind (69.9% if handled, 26.3% for open vocabulary alone); the functor over the same text labelled nothing |
| E1 ✓ | how calibrated is Jev per switch today? | switches whose truth is known: labelled demo cases (people's answers do not exist yet) | labelled record export, calibration metrics. **Result** (`deney/e1-jev-calibration`): 144/145 decisions right, 2889 switches, ECE 0.003; but only 21 switches lay between p 0.1 and 0.9, so these labels cannot measure calibration: it needs boundary-dense cases (E2). The one disagreement was a label a person had doubted, at confidence 0.54–0.62 on a 0.6 gate (followed once, escalated twice) |
| E2 ✓ | does resonance label synthetic cases correctly? | synthetic cases through path equations, functor squares, ensembles | synthetic generator, resonance checker. **Result** (`deney/e2-boundary-resonance`): boundary cases triple uncertain calls (24% vs 8%); a loop of two readings of the same text by the same model labels nothing (0 of 6 disagreements flagged: errors correlate), so only loops over **different information** can be label sources, and E0's resonance count is overstated; the loop's surprises found a policy contradiction instead (UI bugs: `ByTeam` says Product, the Teams description says Technical); synthetic labels must be adjudicated against the whole frame |
| E3 ✓ | do loops over **different information** catch errors? | generated world states, three channels (message, records, event log), independent vs correlated noise | ensembles with per-column `state`. **Result** (`deney/e3-independent-perspectives`): undetected error falls as **U ≈ ē^k_eff**; with independent channels k_eff ≈ k (slope −1.08 against ln ē = −1.11; three perspectives worth 2.97), with correlated channels k_eff ≈ 1 (slope −0.11); one shared judge added no correlation; the price is coverage (60% of cases checked by all three) |
| E4 ✓ | does a CPU switch match Jev's calibration on admitted switches, and at what latency and cost? | E1's labels plus E2's | the learner. **Result** (`deney/e4-cpu-switch`): a hashed-feature logistic learner on numpy matched or beat Jev on structured switches at ~1/10,000 of the latency; on free text it took 77% of cases under a gate with the system at Jev's accuracy (99.4%), distillation without truth as good as truth; open vocabulary failed (1.9%, below chance) |
| E5 ✓ | does the self-building loop resolve more intents per model call, under the same calibration and escalation budget? | support-commons, benefits-assembly streams | E1–E3, the objective. **Result** (`deney/e5-self-building-loop`): 1.82× intents per model call but 95.3% accuracy against Jev alone's 99.8%: the learner answered situations it had never seen with confidence (so Jev, and the open world, never saw them) and was overconfident early; a guard (minimum evidence, audits, suspension on new vocabulary) gave 98.3% at 1.30×. The loop needs a novelty signal the learner computes (E5b) |

## 7. What the engine needs for this

Added to the M4 engine work (`docs/10` §4), each useful on its own:

1. **labelled record export**: per switch, the state seen, the answer,
   the outcome, and every label that later arrived (person, evidence,
   resonance) with its source;
2. **calibration metrics** per switch in the trace insights;
3. **a calibration map** per switch, applied before the confidence gate;
4. **a resonance checker** over path equations, functor squares and
   ensembles;
5. **the source of a resume answer** in the record (person, system,
   evidence);
6. **a synthetic case generator** per switch (proposer-driven, bounded,
   marked synthetic in every record).
