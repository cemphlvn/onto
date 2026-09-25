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
| E0 ✓ | **ceiling**: what share of recorded switches the CPU capability admits, before building anything (OSIL's capability-ceiling method) | frame records of every demo run | a switch classifier over records. **Result** (`deney/e0-switch-ceiling`): 55.2% of 737 switches; context length does not bind; open vocabulary and missing labels bind together (99.6% if both are handled, 73.0% and 69.9% alone); adopting a functor made 60% of Support's switches labelable by resonance |
| E1 | how calibrated is Jev per switch today? | switches whose truth is known: attested evidence (incident-response, benefits-assembly, consent-enforcement), people's answers | labelled record export, calibration metrics |
| E2 | does resonance label synthetic cases correctly? | synthetic cases through path equations, functor squares, ensembles | synthetic generator, resonance checker |
| E3 | does a CPU switch match Jev's calibration on admitted switches, and at what latency and cost? | E1's labels plus E2's | the learner |
| E4 | does the self-building loop resolve more intents per model call, under the same calibration and escalation budget? | support-commons, benefits-assembly streams | E1–E3, the objective |

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
