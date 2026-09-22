# onto — Scientific Positioning

Status: **draft 2 — 2026-09-23.** Where onto sits among research fields,
ranked by what the runtime **demonstrably does today** (disposition records included,
live runs in `demos/`), separated from what it could become. Update this
file when a capability lands; do not promote a field on intention alone.

## 1. One-line identity

**Today:**

> onto is a concurrent guarded-transition runtime whose guards are code or
> calibrated models; it defers to generative models at the boundary of its
> local closed-world claims, and records the provenance of every decision.

Short form: *a model-judged, provenance-carrying, concurrent transition
runtime with open-world escalation.*

**After M2 (not yet):**

> a potentiality-preserving supervisory runtime that governs both the
> execution and the extension of an open transition system.

"Judgment–disposition graph runtime" now describes half of it: the
disposition graph exists (frame records linked by `after`, §4); the
supervisory half (M2) does not.

## 2. What the runtime is, precisely

```mermaid
flowchart TD
    S[case state<br/>goal + JSON facts + hops] --> F[decision frame<br/>outgoing arrows of the current object]
    F --> R{require<br/>code guard}
    R -->|eligible arrows| C[claim frame<br/>read / write, per policy]
    C --> J[judge: Choice · Noul · Score<br/>one request, calibrated]
    J -->|confident| D{traversal rule}
    D -->|one arrow| N[next object]
    D -->|noul: independent aspects| K[fork into branch walks]
    D -->|noul: competing readings| A[follow best · log alternatives]
    J -->|declines or unsure| E[escalate: none_of_these / open_frame / low_confidence]
    R -->|nothing eligible| E
    E --> P[System 2 proposer<br/>sees pending proposals at this frame]
    P --> V[provisional proposals<br/>never promoted yet]
    C -.intersections.-> Q[potentialities<br/>node · conceptual · alternative]
    N & K & A & V & Q --> T[provenance<br/>disposition records (DAG) + JSONL events + report]
```

Two corrections to the flow diagram we were sent:

- **`require` comes before judgment, not after.** Code removes transitions
  whose preconditions fail; the model only judges what code left enabled.
  Judgment then *selects* among enabled transitions. (If nothing is
  enabled, no model is called: tested in `require_blocks_arrows_without_evidence`.)
- **Frame claims and potentialities are missing from it**, and they are
  the concurrency half of the system.

## 3. Ranked fields

Fit: **direct** (the runtime is an instance), **strong** (a precise
analogy with a named gap), **emerging** (one mechanism demonstrated, the
theory not yet respected), **analogy** / **inspiration** / **not yet**.

| # | field | fit today | what matches | what is missing |
|---|---|---|---|---|
| 1 | Guarded transition systems | **direct** | objects = states, arrows = labelled transitions, walks = traces; guards of four kinds: `require` (Boolean), Choice (competitive), Noul (independent), Score (ordinal) | guards are not composable formulas; no temporal-logic properties checked |
| 2 | Selective prediction, learning to defer, model cascades | **direct** | calibrated System 1 abstains (`none_of_these`, low confidence) and defers to a larger model; threshold gates on the model's own confidence | deferral target is a *generator of structure*, not a better answerer: that twist is onto's, and is untested against deferral theory (no cost model, no deferral policy learned) |
| 3 | Decision provenance | **direct** | every frame visit is a disposition record: every candidate with judgment, disposition and a deterministic reason; judge call and claim metadata; causal `after` links forming a DAG across forks (`--dispositions`, `onto why`) | not yet in W3C PROV vocabulary; records written at run end, not streamed |
| 4 | Local closed-world reasoning (open/closed-world KR) | **strong** | `closed:` is a local closed-world assertion ("these arrows are all the cases"); open frames are open-world; `none_of_these` at a closed frame is evidence the assertion failed, `open_frame` an expected gap | assertions are not revised on evidence; no aggregation of LCW violations into taxonomy repair |
| 5 | Workflow nets / Petri nets | **strong analogy** | walk ≈ token, object ≈ place, arrow ≈ transition, `require` ≈ enabling condition, noul fork ≈ token split, frame claim ≈ shared-resource constraint | no marking or capacity, **no joins** (branches never synchronise), no soundness / deadlock analysis |
| 6 | Neurosymbolic AI | **strong (classification)** | Kautz's *Symbolic[Neuro]*: a symbolic control loop (typed graph, code guards, path equality) that calls neural judgments as subroutines | no learning flows back into the symbolic side yet (M2 promotion would be the first) |
| 7 | Category theory and rewriting | **structural foundation** | a category presented by generators and relations; typed composition; path equality by equality saturation (egg), answering Unknown where the word problem is undecidable | no functors or universal constructions; categorical semantics of the *dynamics* is open |
| 8 | Event structures / concurrency semantics | **emerging** | causality (the `after` DAG of frame records, across forks), concurrency (coexisted claims), and one mechanism that is exactly the conflict/concurrency split: the **fork Noul judges whether two enabled transitions are independent (concurrent) or competing (in conflict)** | node intersection is not formal conflict; no event-structure semantics defined or checked |
| 9 | Supervisory control of discrete-event systems | **emerging** | `require` disables transitions, claims restrict interleavings, low confidence withholds a transition, proposals are not admitted | no supervisor proves anything; the consent runs show unsafe proposals (`Collected → Marketing` twice) held back only by *non-promotion*. M2 makes this real, and adds a rare twist: supervising the **extension** of the plant, not only its execution |
| 10 | Agent-workflow frameworks (graph-of-LLM-calls orchestrators) | **practical neighbour** | graph of nodes, model-driven conditional edges, parallel branches | not a research field; onto's differences are the positioning: typed paths + equations, calibrated abstention, frame-level concurrency claims, potentiality log, governed open-world extension |
| 11 | Planning under uncertainty | **analogy** | probabilities steer traversal | no reward, utility, transition model or objective; not decision-theoretic |
| 12 | Truth maintenance (ATMS) | **weak today** | provisional proposals are assumptions; alternatives are logged | no maintained assumption environments or consistent "possible worlds"; would matter for the potentiality-preserving stage |
| 13 | Computational argumentation | **weak today** | candidates and rationales exist | no attack/support relations between candidates |
| 14 | Thousand Brains / cortical columns | **inspiration** | "a frame is a local decision unit" | no reference frames, sensorimotor learning or column voting; do not claim a model of cortex |
| 15 | Active inference | **not applicable** | — | no generative model, prediction error or free-energy objective |
| 16 | Adaptive / optimal control | **not yet** | — | no plant model, feedback law or objective |

**Folded in or re-scoped from the earlier list:**

- *Calibrated decision models (RLCD)* → part of #2. Calibration is what
  makes threshold-based deferral principled; onto relies on it, does not
  contribute to it.
- *E-graphs and equality saturation* → the implementation of #7.
- *Neurosymbolic AI and knowledge graphs* → #6. onto is not a knowledge
  graph of instances; it is schema-level. Its `.onto` presentations are
  close kin to **ologs** (Spivak & Kent) in syntax (typed objects, labelled
  arrows, commutative path equations), but olog arrows are *functional
  relations between types* and onto arrows are *state transitions*. Same
  notation, different semantics: worth saying explicitly in papers.
- *The Corballis recursion line* → design rationale (docs/00 §4), not a
  scientific positioning. The testable claim inside it is #2: a
  non-generative base with generative deferral.

## 4. The next claims, and what earns them

| claim | earned by | field it upgrades |
|---|---|---|
| decisions are fully accountable | **disposition records** — **done** (`a2128ae` + this change): every candidate, judgment, disposition, reason; first-class artifact | #3 → direct ✓; groundwork for #12 |
| unsafe structure cannot enter | **M2 supervisor**: typing and composition → declared invariants (e.g. every path to Marketing passes Consented) → reachability restrictions → semantic duplicate/overlap (Jev Noul) → admit · reject · unknown | #9 → strong; #4 gains revision |
| concurrent branches are a process, not just parallel walks | **joins** (synchronise branches at an object) and a marking | #5 → direct; #8 gains a semantics |
| potentialities have a theory | define conflict and concurrency over events (not nodes) and check them | #8 → strong |
| legal questions get proofs | map the question to a target use (judge), then prove reachability / non-reachability with `reach --avoid` over `require`-filtered structure | #1, #4, #9 together; fixes the consent regression below |

## 5. Honest boundary (from the live runs)

- **Not decision theory.** Choice probabilities select; nothing maximises
  expected utility.
- **Not proof-based consent reasoning yet.** After descriptions, 6 of 6
  original consent uses escalated at `Collected` (4 of 6 before). Local
  judgment became stricter and more correct (Jev will not assume a legal
  basis from the use alone), but no reasoning about reachability happened.
  The right mechanism is §4 row 5, not better descriptions.
- **Potentialities are runtime observations,** not semantic conflicts. Two
  walks touching `Network` may be fine; two proposals touching different
  names may conflict.
- **One fork demonstrated live** (incident-graph, p = 0.92). Branch
  behaviour is otherwise tested only with mocks.

## 6. Pointers

Guarded commands: Dijkstra 1975. Probabilistic automata: Segala 1995.
Reject option: Chow 1970; selective classification: Geifman & El-Yaniv
2017; learning to defer: Madras, Pitassi & Zemel 2018; LLM cascades:
Chen, Zaharia & Zou 2023 (FrugalGPT). Decision provenance: Singh, Cobbe &
Norval 2019; W3C PROV: Moreau & Groth 2013. Local closed-world reasoning:
Etzioni, Golden & Weld 1994. Workflow nets: van der Aalst 1997.
Neurosymbolic taxonomy: Kautz 2020/2022. Ologs: Spivak & Kent 2012.
Equality saturation / egg: Willsey et al. 2021. Event structures:
Nielsen, Plotkin & Winskel 1981; Winskel 1987. Supervisory control:
Ramadge & Wonham 1987. ATMS: de Kleer 1986. Abstract argumentation:
Dung 1995. Thousand Brains: Hawkins 2019. The Recursive Mind: Corballis 2011.
(Citations from memory; verify before any publication.)
