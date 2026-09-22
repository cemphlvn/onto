# onto — Architecture

Status: **draft 2 — 2026-09-23.** Single source of truth for how onto is
built. Code follows this document; change it here first.

## 1. What onto is

A category engine that decision models walk. A **category** is a set of
objects and composable arrows (morphisms) with declared path equations. A
**walker** starts at an object and repeatedly picks an outgoing arrow.
Most picks come from a fast, calibrated **System-1** model (e.g. TypeSafe's
Jev, trained with RLCD). When the category does not enumerate the options
well enough, the step escalates to a **System-2** language model, which may
propose new structure.

## 2. Vocabulary

| term | meaning | in code |
|---|---|---|
| object | a category / concept / location | `ObjId`, `Object` |
| arrow (pointer) | a morphism `A -> B` | `ArrowId`, `Arrow` |
| path | a composite `h.g.f` (f first) | `Path` |
| decision frame / router index | an object plus its outgoing arrows; the unit a System-1 model decides over (cf. a cortical column) | `Category::out` |
| closed frame | outgoing arrows verified MECE (mutually exclusive, collectively exhaustive) | `Closure::Closed` |
| open frame | known to be incomplete | `Closure::Open` |
| equation | two parallel paths declared equal | `Equation` |
| frame primitive | how a frame is decided: `choice` (one arrow), `noul` (each arrow's condition on its own), `score` (one ordered scale; arrows are levels) | `Primitive`, `Frame` |
| instructions | an arrow's meaning, text or structured JSON; rendered per primitive by each model adapter | `Arrow::instructions` |
| `about` | an object's meaning | `Object::about` |
| `require` | a structured precondition checked in code against the case's JSON facts before any model is asked | `Require` |
| judge | the System-1 model interface (was "chooser"): answers a frame with the frame's primitive | `walk::Judge`, `model::Judge` |
| fork | a noul frame where several arrows hold and the judge says they are independent aspects: the walk splits into parallel branches | `Decision::Fork` |
| parallel arrows | several arrows with the same endpoints and different meaning or preconditions (e.g. two legal bases) | — |
| footprint | a frame's object plus its arrows' targets: every node one decision could touch | `frames::Claim` |
| claim | a walk's hold on a footprint while deciding, `read` (System 1) or `write` (System 2) | `frames::Claim` |
| potentiality | a logged place where two concurrent walks could meet (node or conceptual) | `frames::Potentiality` |

## 3. Layers

```
        Jev / OpenJev  (System 1, RLCD-calibrated)      LLM (System 2)
                │ Judge: choice | noul | score over the frame  │ Proposer
                ▼                                               ▼
┌──────────────────── onto-runtime (Rust, tokio) ────────────────────┐
│ engine     one task per walk, parallel model calls    [M1.5 ✓]    │
│ frames     footprint claims, policies, potentialities [M1.5 ✓]    │
│ providers  Jev (TypeSafe) judge, OpenRouter proposer  [M1.5 ✓]   │
│ telemetry  JSON lines · mem: heap counter + RSS       [M1.5 ✓]    │
└───────────────────────────────┬───────────────────────────────────┘
┌──────────────────────── onto-core (Rust) ─────────────────────────┐
│ category   objects + arrows, u32 ids, CSR frames      [M1 ✓]      │
│ path       type-checked composition                   [M1 ✓]      │
│ equality   laws + equations as egg rewrites           [M1 ✓]      │
│ walk       two-tier stepper, provisional proposals    [M1 ✓]      │
│ delta      append-only log of verified additions      [M2]        │
│ snapshot   rkyv + mmap, zero-copy load                [M2]        │
│ functor    structure-preserving maps between cats     [M3]        │
└──────────┬────────────────────┬──────────────────────┬────────────┘
     C ABI (cbindgen) [M4]   PyO3 [M4]           OSIL bridge [M5]
     C/C++ consumers         Jev SDK users        osil-opt (shared egg)
```

`onto-core` stays synchronous and light (egg, thiserror; serde optional)
so it can sit behind a C ABI. `onto-runtime` adds the async loop and the
network clients. `onto-cli` (the `onto` binary) is a thin shell over both.

## 4. The two-tier step

```
step(case, A):
  frame = arrows out of A whose `require` holds in case     (code, no model)
  if A is open:                     escalate (open_frame)
  if frame is empty:                escalate (none_of_these)
  answer = Judge(frame, A.frame.primitive, hops so far)     (one request)
    choice: one arrow or none_of_these      → follow, or escalate
    noul:   P(holds) per arrow (+ P(fork))  → none: escalate; one: follow;
            several: fork into branches if the judge says "independent
            aspects" and the branch budget allows, else follow the best and
            log the rest as alternatives
    score:  P(level)                        → follow the level's arrow
  low confidence                    → escalate (low_confidence)
  escalate → Proposer → proposals (with `about`), recorded as provisional
```

Rules:

1. **"None of these" is always a candidate.** A System-1 model must be able
   to say the frame does not fit (TypeSafe guidance: include a no-match
   outcome).
2. **Proposals never enter the category directly.** They are provisional
   until verified: well-typed, law-consistent (egg), and not overlapping
   existing arrows (a pairwise MECE check, itself a System-1 judgment).
3. **Verified proposals close gaps permanently**, so the fast path covers
   more over time. (Mechanism: M2 delta log.)
4. A closed frame with no outgoing arrows is a **terminal**; the walk stops.
5. **The gate is the model's own confidence** when it reports one (Jev
   does), else the top probability. One function, `walk::decide`, holds
   this rule for every walker.
6. **Meaning is stored once; adapters render it.** The graph holds
   `about`, instructions, levels and `require`. Each provider's adapter
   turns a frame into its own input: for Jev, a Choice with instructions as
   criteria, one Noul per arrow plus the fork Noul, or a Score with the
   levels as ordered criteria, all in one request (TypeSafe evaluates
   questions in parallel, so extra questions cost tokens, not latency).
7. **Fork or follow-best is itself a judgment.** In a noul frame with
   several arrows the judge is also asked, in the same request and given
   the hops so far, whether they are independent aspects (fork) or
   competing readings (follow the best). Code guards it: the question is
   only asked while the job's branch budget (`max_branches`, default 4)
   and fork depth (`max_fork_depth`, default 2) allow. Branches run as
   their own walks with their own claims; their intersections are logged
   like any other.
8. **Evidence before judgment.** `require` runs in code first; a missing
   field fails its clause, so an arrow never opens on absent evidence, and
   no model is asked when code has already ruled every arrow out.

Rationale. Following Corballis (*The Recursive Mind*, 2011), recursion is
treated as a separable capability layered on a non-recursive base: the
graph routes; the language model generates nested structure only when the
graph runs out. Everett's claim that Pirahã lacks recursion is disputed
(Nevins, Pesetsky & Rodrigues 2009); the design depends only on recursion
being separable, not on that claim.

## 5. The core loop (onto-runtime)

Many walks run at once, one tokio task each. Model calls from different
walks run in parallel, bounded per provider (default 16 in flight for the
judge, 4 for the proposer). Each step:

```
claim footprint(A)            read at a closed frame, write at an open one
  closed: Judge (Jev)         → follow or fork, release, next step
          else escalate:      upgrade read → write, Proposer (OpenRouter)
  open:   Proposer
release; record proposals (provisional) and conceptual intersections
```

### 5.1 Claims and policies

A walk holds at most one claim, taken all-or-nothing over the footprint,
so claims cannot deadlock. Whether two intersecting claims wait:

| policy | waits when | use |
|---|---|---|
| `exclusive` (default) | footprints share any node | decision frames that could converge are decided one after another |
| `shared` | same frame, and one claim is a write | readers of an unchanged frame run together; only frame edits serialize |

Every intersection is logged as a **potentiality**, waited on or not:

- `node`: two footprints share nodes (`resolution`: `waited` or `coexisted`).
- `conceptual`: two walks proposed the same new concept (name-normalized:
  `Password_Reset` = `passwordreset`), or the same arrow into an existing
  object. These are candidates for merging during verification (M2).

### 5.2 Speculation

`--speculate` starts the System-2 call alongside System 1 at every closed
frame and aborts it when System 1 is confident. It saves one round trip on
escalation but claims every frame as a write, so under `shared` it gives up
reader parallelism. Measured below.

### 5.3 Telemetry

JSON lines, one event per line, only the `onto` target (no dependency
logs). Every line has `timestamp`, `level`, `event`:

| event | fields |
|---|---|
| `run.start` | category, walks, judge, proposer, policy, speculate, threshold, max_branches, max_fork_depth |
| `walk.start` / `walk.end` | walk, parent, from, goal / path, steps, elapsed_ms |
| `judge.call` | walk, at, primitive, questions, latency_ms, top_p, holds, fork_p, confidence, input_tokens, output_tokens, attempts, ok (error on failure) |
| `fork` | walk, at, fork_p, branches, spawned |
| `proposer.call` | walk, at, speculative, latency_ms, proposals, input_tokens, output_tokens, attempts, ok |
| `proposer.discarded` | walk, at (speculative call aborted) |
| `step` / `escalate` | walk, from, arrow, to, decided_by, p, confidence, alternatives / walk, at, reason |
| `potentiality` | kind (node, conceptual, alternative), resolution (waited, coexisted, not_followed), mode, walk, with, at, nodes (comma-joined), wait_ms |
| `mem.sample` | rss_bytes, heap_bytes, heap_peak_bytes (every 250 ms) |
| `run.end` | wall_ms, model_ms_sum, judge_calls, judge_questions, proposer_calls, forks, branches, tokens, potentialities, peak_rss_bytes, heap_peak_bytes |

Memory: heap bytes are exact, from a counting global allocator the binary
installs (`mem::CountingAlloc`); RSS comes from the OS (`memory-stats`).
`--report` writes the whole run (walks, steps, proposals, potentialities,
memory) as one JSON document.

### 5.4 Measured (2026-09-23, 6 walks, `examples/triage.jobs`, release build)

| run | wall | model time | parallelism | waited | peak RSS | peak heap |
|---|---|---|---|---|---|---|
| exclusive | 16.9 s | 18.7 s | 1.11x | 19 | 13.9 MiB | 0.34 MiB |
| shared | 9.2 s | 22.9 s | 2.50x | 2 | 14.8 MiB | 0.45 MiB |
| shared + speculate | 10.5 s | 19.8 s | 1.89x | 10 | 14.1 MiB | 0.46 MiB |

Judge `jev-latest` (~0.7 s/call), proposer `~openai/gpt-luna-latest`
(~1.7–3 s/call). All six walks start at `Request`, so `exclusive`
serializes them almost fully. The engine itself is small; RSS growth is
mostly the TLS/HTTP stack.

## 6. Path equality

Paths become egg terms: `g.f` is `(o g f)`, identities are `(id A)`.
Rewrites: associativity (both directions), left/right identity, and every
declared equation (both directions). Only well-typed terms enter the
e-graph, so the identity rewrites need no side conditions.

The word problem for finitely presented categories is undecidable, so
`Equality::check` returns `Equal`, `Distinct` (saturated without merging),
or `Unknown` (limits hit). It never guesses.

## 7. The `.onto` format

```
category Name {
    objects: A, B, C, D;
    about A: "what A means";                   # text or {JSON}
    frame A: choice "Which way?";              # choice | noul | score, optional question
    f: A -> B "when to take f";                # instructions: "text", {object} or [array]
    g: A -> C {"meaning": "...", "examples": ["..."]};
    h: A -> D "..." require case.flag == true and case.n >= 2;
    closed: A;                                 # A's frame is MECE (choice) / exhaustive
    frame C: score "How severe?";
    low:  C -> B level 0 "no impact";          # score frames: one arrow per level
    high: C -> D level 1 "blocking";
    g2.f = h2;                                 # path equation
    inv.f = id(A);                             # identities
}
```

Statements end with `;` (outside strings and JSON), `#` starts a comment
(outside strings), objects are declared before use. After `Src -> Dst` an
arrow takes, in any order, `level N` and an instruction, then optionally
`require EXPR` last. `require`: dot paths into the case JSON, `== != < <=
> >=` against JSON literals, joined by `and`; a bare path must be truthy.
Jobs are `Start: text` or `Start: {"goal": "...", ...facts}`. `o`, `id` and `none_of_these` are reserved names. Object and arrow names share one
namespace.

## 8. Storage

The hot operation is a step: read one frame, choose, compose. Frames are
stored CSR (one contiguous slice per object). M1 keeps the category in
memory. M2 adds an immutable rkyv snapshot loaded by mmap, plus an
append-only delta log for verified additions, compacted into a new
snapshot. No database until live multi-writer editing is needed.

## 9. Decisions

| # | decision | why |
|---|---|---|
| D1 | Rust (edition 2024) | memory safety for LF security review; same `egg` as OSIL; C ABI + PyO3 reach C/C++ and Python |
| D2 | pointer = arrow; object + outgoing arrows = decision frame | direct fit for a Choice over a closed option set |
| D3 | standalone project, OSIL bridge later | separate release cycle; share egg-level machinery |
| D4 | MIT + DCO, no CLA | matches OSIL; OSI-approved (no explicit patent grant; revisit before foundation submission) |
| D5 | in-memory CSR now, rkyv/mmap snapshot next | the step is the hot path |
| D6 | async runtime in its own crate (tokio, reqwest+rustls) | keeps `onto-core` sync and C-ABI friendly |
| D7 | Jev over the plain HTTP API, OpenRouter via chat completions with JSON-schema output | no SDK in Rust; both are one POST |
| D8 | default policy `exclusive`; `shared` opt-in | follows the stated method (intersecting frames wait); `shared` measured faster |
| D9 | proposer default `~openai/gpt-luna-latest`, `reasoning.effort = low` | reasoning models otherwise spend the token budget and return no content |
| D10 | telemetry as JSON lines via `tracing`; heap via counting allocator | greppable, `jq`-able, exact heap numbers |
| D11 | frames declare a primitive (choice/noul/score); arrows carry instructions (text or JSON) | meaning is not tied to one question type; Jev evaluates all three natively |
| D12 | fork vs follow-best decided per frame by a Noul over state and hops, guarded by a code budget | the case decides, within limits code enforces |
| D13 | `require` preconditions evaluated in code over case JSON | known rules stay deterministic; evidence gates judgment |
| D14 | System-1 interface renamed Chooser → Judge; telemetry `chooser.call` → `judge.call` | it no longer only chooses |

## 10. Milestones

- **M1 (done):** core store, typed paths, egg equality, two-tier walker with
  scripted/uniform choosers, `.onto` parser, `onto` CLI.
- **M1.5 (done):** async core loop, frame claims + potentialities, Jev
  and OpenRouter clients, speculation, JSON-lines telemetry, memory
  measurement, `onto run`.
- **Meaning (done):** `about`, arrow instructions, frame primitives
  (choice/noul/score), score levels, parallel arrows, `require`, noul
  forks with a judged fork decision, Jev adapter per primitive.
- **Demos:** `demos/` (Onto Commons); wave 1 runnable: support-commons,
  incident-graph, consent-paths. `onto reach` lists routes grouped by
  equality, with `--avoid`.
- **M2:** delta log + proposal verification pipeline; rkyv/mmap snapshot.
- **M3:** functors between categories; multi-category files.
- **M4:** C ABI (cbindgen) and Python bindings (PyO3); Jev `Chooser` adapter
  (Choice primitive over the frame) in Python.
- **M5:** OSIL bridge: onto categories as OSIL category-level requirements.

## 11. Open questions

- How frames should treat arrows that are *derived* by an equation (should
  a defined composite appear as a choice?).
- What the pairwise MECE verifier asks the System-1 model, exactly.
- Fairness under `shared`: a steady stream of readers can delay a writer
  at the same frame. No starvation seen at this scale; a writer-preference
  queue is the fix if it appears.
- Conceptual intersection is name-based. A Jev Noul ("do these two
  proposals denote the same concept?") would catch synonyms (`Refund` vs
  `ChargeDispute`).
- Proposers sometimes re-propose arrows that already exist (`plan`,
  `specify` from `Feature`); M2 verification must drop them. Frames that
  keep attracting such proposals are candidates for closing.
- **Invariants for M2 verification** (from the consent-paths demo): a
  proposer suggested `Collected -> Marketing`, bypassing consent. The
  format needs declared invariants (e.g. "every path to Marketing passes
  Consented", checkable with `Category::paths(.., avoid, ..)`) that every
  proposal must preserve before promotion.
- **Open frames always escalate**, even when an existing arrow fits (with
  descriptions, "password reset email never arrives" matched `login`
  exactly and still escalated). Proposal: judge open frames too; escalate
  when the judge declines, is unsure, or the frame has no arrows.
- **Waiting at a frame buys nothing yet**: a walk waits for another walk's
  System-2 call at the same frame, but never sees its proposals. Passing
  the frame's pending provisional proposals to the next proposer would turn
  the wait into deduplication at the source (and fix name-based grouping).
- **Legal bases are evidence, not inference** (consent-paths): judged from
  the intended use alone, Jev rightly declines to assume consent or
  contract. Bases belong in `require` against case facts.
- Jev Choice holds at most 255 options; wider frames need hierarchical
  (beam) choice.
