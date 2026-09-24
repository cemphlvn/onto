# onto — Call Economy: Which Model Calls a Case Waits For

Status: **steps 0–3 built and measured live — 2026-09-24**. Follows
`docs/07-raster-findings.md`, which found that 42–84% of proposer time
went where no structure could be learned, and that proposer calls held
frame claims other cases waited on.

The principle: **a walk that cannot continue does not always need a new
arrow.** A model call belongs on a case's path only if its result can
change that case's next state. Everything else is the system's learning
(curation, off the path) or a person's decision (review).

## 1. Typed escalations (step 1)

Every escalation is classified from the frame record, the frame's
admission and the budget (`onto_core::walk::EscalationKind`):

| kind | when | route |
|---|---|---|
| **structure gap** | the known options do not cover the case, where structure may be learned | transport, then the proposer, on the case's path |
| **evidence gap** | an option was held back by a missing attestation, precondition or entry contract, and no eligible option fitted | no model: new structure could only bypass the evidence; the record names what is missing |
| **policy stop** | a sealed frame; a join a sibling left | a **gap signal** for curation; the case does not wait |
| **budget stop** | branch or expansion budget exhausted | stop |

A structure gap in a closed-world run is also routed to curation: its
proposals are only for a person. `onto ask` (an interactive session)
and `onto run --propose-inline` keep computing review proposals inline.

Each escalation gets a **gap key**: category snapshot + frame + kind +
the missing distinction (the missing evidence for an evidence gap; the
reason and the branch focus for a structure gap). Frame records carry
it; telemetry writes a `gap` event with the route taken.

## 2. MVCC and single-flight (step 2)

- The frame claim is released **before** any model call (transport's
  critic, the proposer). Walks read the graph version current when they
  step; a reader never waits for a proposer.
- **Single-flight**: one proposal in flight per gap key. A walk meeting
  the same gap subscribes to the answer instead of calling; if the
  answer taught the graph something, it re-judges the frame, otherwise
  it records the leader's proposals as provisional. Subscriptions are
  recorded as conceptual intersections, as before.
- **Admission re-validates**: the lock is gone, the checks are not. After
  the model answers, the proposals are reviewed against the current
  graph and admitted under the graph's write lock, where every
  structural proof runs again against the version being extended. If
  the graph changed while the model answered, a `gap.stale` event
  records it.

## 3. Curation queue (step 3)

Policy stops and closed-world structure gaps become `GapSignal`s
(`<stem>.gaps.jsonl`): gap key, kind, frame, the missing distinction,
the frame's options, and the case **as the frame's state policy showed
it to a model** (never the raw case). Then:

```
onto run … ──► support.gaps.jsonl ──► onto curate support.onto ──► support.curated.jsonl
                                        (one proposal per gap,        ──► onto review ──► onto promote (a person)
                                         ≤ 3 representative cases)
```

Live (support-commons, two runs of the parcel-problem stream): 14
signals in 5 gaps, 5 proposer calls (inline, 14), the main gap (8
signals, Ticket) answered with `shipping: Ticket -> Shipping` — the
missing team. Review: 5 unknown (every proposal challenges a closed
frame), so a person decides.

## 4. Measurements (step 0 and after each step)

Call economy is part of the raster's insights (`onto raster --json`):
proposer calls by kind, **needed** (structure gap, learnable), **deferrable**
(review only), **unnecessary** (evidence or budget), repeats per gap,
calls **avoided** by single-flight, fan-out, stale results, learned /
used / held, utility ((learned and used + held) / calls), and proposer
time per resolved gap. The same seven scenarios, live:

| scenario | proposer calls (base → 1 → 2) | proposer time | unnecessary time | claim waits | wall |
|---|---|---|---|---|---|
| 1 incident-response | 9 → 2 → **1** (1 avoided) | 50.2 → 9.1 → **8.6 s** | 36.0 → **0** | 0 | 17.2 → 10.8 → **10.2 s** |
| 2 support 40, exclusive | 0 | 0 | 0 | 60.1 → 55.6 s (reader policy) | 7.8 → 7.5 s |
| 2 support 40, shared | 0 | 0 | 0 | 0 | 2.9 → 2.8 s |
| 3 infrastructure change | 5 → 1 → **1** | 22.5 → 3.7 → 4.4 s | 18.4 → **0** | 16.0 → **0** | 11.6 → **4.9** → 5.4 s |
| 4 hospital discharge | 6 → 5 → **3** (2 avoided) | 36.1 → 24.4 → **18.3 s** | 6.8 → **0** | 0 | 16.9 → 21.3 → **14.2 s** |
| 5 consent, exclusive | 8 → 3 → 3 | 26.6 → 11.8 → 10.2 s | 15.2 → **0** | 46.3 → 15.2 → 12.9 s | 19.8 → 15.0 → **13.5 s** |
| 5 consent, shared | 8 → 3 → 3 | 27.9 → 11.6 → 13.2 s | 14.9 → **0** | 17.5 → **0** | 19.1 → **13.4** → 15.1 s |
| 6 repeated gaps | 7 → **0** (to curation) | 27.9 → **0** | 0 | 24.2 → **0** | 20.4 → **11.4** → 11.3 s |
| 7 learning (5 s arrivals) | 24 → 7 → **5** | 98.7 → 37.5 → **21.8 s** | 73.2 → **0** | 4.8 → **0** | 36.5 → 36.4 → 35.1 s |

Reading it honestly:

- **Typed escalations removed every unnecessary call** (up to 73 s per
  run) and every claim wait caused by proposers.
- **Single-flight** saved calls where cases met the same gap at once
  (hospital: 2 of 5; incident: 1).
- Wall time fell by 30–58% where proposers had been on the critical path;
  scenario 7 is bounded by its arrival stream (7 cases × 5 s), so its
  wall time barely moves while its proposer time fell 78%.
- Model latency varies run to run: single runs, not statistics
  (hospital's wall *rose* after step 1 before falling after step 2).
- The remaining claim waits are the **exclusive reader policy** (2, 5),
  a policy choice, not a proposer effect.

## 5. What is not automatic (steps 4–5)

The measurements produce **recommendations**, never automatic changes to
admission or safety policy. Candidates the numbers support today:
`shared` for entry frames (2: 3.6× faster), retiring learned arrows that
are never used (07 §1), and looking at frames whose gaps keep coming
back (6: one team missing).
