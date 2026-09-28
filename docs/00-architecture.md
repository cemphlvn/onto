# onto — Architecture

Status: **draft 8 — 2026-09-24** (audited before M4). Single source of truth for how onto is
built. Code follows this document; change it here first.

## 1. What onto is

A category engine that decision models walk. A **category** is a set of
objects and composable arrows (morphisms) with declared path equations. A
**walker** starts at an object and repeatedly picks an outgoing arrow.
Most picks come from a fast, calibrated **System-1** model (e.g. TypeSafe's
Jev, trained with RLCD). When the category does not enumerate the options
well enough, the step escalates, and the escalation is typed: structure
may be recalled from a library of learned arrows, transported from
another category through a functor, or proposed by a **System-2**
language model; missing evidence, a sealed frame or a spent budget stop
without a model, for a person or for curation.

Two layers of structure: the **declared** policy (`.onto`, written only
by people) and the **learned** layer (admitted by the open world under a
per-frame admission regime, replayed through the proofs on every load).
Several categories relate through **functors** (views, standards,
versions, transport, discovery) and can walk one case together as an
**ensemble**, compared in a shared category.

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
| frame primitive | how a frame is decided: `choice` (one arrow), `noul` (each arrow's condition on its own; `noul parallel` pursues every holding arrow as a concurrent alternative), `score` (one ordered scale; arrows are levels), `split` (no judgment: every eligible arrow is pursued) | `Primitive`, `Frame` |
| instructions | an arrow's meaning, text or structured JSON; rendered per primitive by each model adapter | `Arrow::instructions` |
| `about` | an object's meaning | `Object::about` |
| `require` | a structured precondition checked in code against the case's JSON facts before any model is asked | `Require` |
| judge | the System-1 model interface (was "chooser"): answers a frame with the frame's primitive | `walk::Judge`, `model::Judge` |
| fork | a noul frame where several arrows hold and the judge says they are independent aspects: the walk splits into parallel branches | `Decision::Fork` |
| parallel arrows | several arrows with the same endpoints and different meaning or preconditions (e.g. two legal bases) | — |
| disposition | what happened to one candidate arrow at a frame visit: selected · forked · alternative · rejected · deferred · filtered_by_require, with the judge's number and a reason built from the numbers | `walk::Disposition`, `walk::dispose` |
| capability token | an on/off property a walk holds, set by `ensures` and cleared by `revokes` on arrows; a capability, not a fact: an arrow standing for a real-world fact must `require` the evidence before it `ensures` the token | `Arrow::ensures`, `WalkState::tokens` |
| capability declaration | which arrows may issue and revoke a token (`capability T { issuers: …; revokers: …; }`); every token used must be declared | `Capability` |
| behavioural quotient | coarsest bisimulation over frames (labels without names, set semantics), exact or structure-only; diagnostics, never applied | `quotient.rs`, `onto quotient` |
| attested observation | an Ed25519-signed claim about the world by a declared attester, bound to a case; `attested P` preconditions read only verified ones | `attest.rs`, `onto attest` |
| join | where sibling branches of one fork recombine: `all` (intersection of tokens), `race` (first arrival), `gate` (authority exports an allowlist); `incomplete_join` / `blocked_by_gate` when a sibling cannot arrive | `Join`, `joins.rs` |
| declared root | an application entry point (`start: A;`); starting still counts as entering | `Category::starts` |
| entry contract | what every walk entering an object must hold (`needs` tokens, `require` over the case); inherited by every incoming arrow, present and future; starting at an object counts as entering it | `Entry`, `Gate` |
| derived law | a statement implied by contracts and effects, e.g. "every walk into Research has taken consent or contract, and no withdraw since" | `laws::derive`, `onto laws` |
| invariant | a rule every extension must keep: `via` (every path passes one of a set), `never` (no path) — proved; `rule` (natural language) — judged | `Invariant` |
| review | the supervisor's verdict on a proposal: every check (well_formed, invariant, rule, duplicate, overlap) with outcome and reason, then admit · reject · unknown | `supervise::Check`, `supervisor::Review` |
| promotion | the human step: writing a reviewed proposal into the `.onto` file with a provenance comment | `onto promote` |
| frame record | one frame visit: claim, judge call, every candidate's disposition, outcome, proposals; `after` links records into a causal DAG | `record::FrameRecord` |
| footprint | a frame's object plus its arrows' targets: every node one decision could touch | `frames::Claim` |
| claim | a walk's hold on a footprint while deciding, `read` (System 1) or `write` (System 2) | `frames::Claim` |
| potentiality | a logged place where two concurrent walks could meet (node or conceptual) | `frames::Potentiality` |
| learned layer | structure the open world admitted, kept apart from policy: `<stem>.learned.jsonl`, replayed through the structural proofs on every load | `engine::Learned`, `onto learned` |
| library state | a learned arrow is `active` (in the graph), `dormant` (out of it, recalled at a gap) or `retired` (a person's decision); never deleted | `engine::ArrowState` |
| admission regime | how new structure may enter at a frame: `open_world` (what no hard check refutes), `assured` (only what every check passed; the rest held for a person), `sealed` (nothing) | `Admission` |
| state policy | what a model is shown at a frame (`state { goal; case: a.b; observed; history; memory; … }`), sectioned asserted / observed / inferred; `invariant unseen` proves a field never reaches a model | `state::StateSpec` |
| precedent | an earlier decision at the same frame for another case, projected onto the frame's current state policy (`memory: similar N`) | `memory::Precedent` |
| escalation kind | why a walk cannot continue: `structure_gap` (library, catalogue, model), `evidence_gap` (no model), `policy_stop` (curation), `budget_stop` | `walk::EscalationKind` |
| gap key / gap signal | the identity of a gap (snapshot + frame + kind + missing distinction); a signal is a gap routed off the case's path to curation, with the case as the frame's state policy showed it | `curation::GapSignal`, `onto curate` |
| module | a file with categories, functors, ensembles and `import`s; `FILE#Name` names one | `parse::Module` |
| functor | a partial map between categories: objects to objects, arrows to paths, equations kept; authority, contracts and invariants reflected | `functor::Functor`, `onto functor` |
| transport | at a structure gap, completing a frame from a functor's empty fibers before any model | `lens.rs` |
| grouped frame | `grouped by F`: a choice made first among the images of the frame's arrows, then within the chosen fiber | `Frame::grouped_by` |
| ensemble | several categories (columns) walking one case independently, their positions compared in a shared category: agreement (one reaches the other), surprise (neither does) | `ensemble.rs`, `onto ensemble` |
| discovery | finding a candidate functor: structure prunes, meaning (a judge) and behaviour (co-visits of the same cases) rank | `discover.rs`, `onto discover` |
| raster event | the trace projection a renderer reads: visits, waits, calls, arrivals, joins, proposals, positions, surprises | `trace::RasterEvent`, `onto raster` |

## 3. Layers

```
   Jev / OpenJev (System 1, RLCD-calibrated)            LLM (System 2)
     │ Judge: choice | noul | score over a frame          │ Proposer
     │ Critic: the supervisor's semantic checks           │
     ▼                                                     ▼
┌──────────────────────── onto-runtime (Rust, tokio) ────────────────────────┐
│ engine      one task per walk; typed escalations: library → transport →    │
│             model; MVCC, single-flight; stale judgments judged again       │
│ frames      footprint claims, policies (shared default), potentialities    │
│ joins       all · race · gate                                              │
│ supervisor  review: structural proofs, then one critic request; held       │
│ record      frame records: the disposition DAG                             │
│ memory      precedents projected onto the state policy                     │
│ lens        grouped frames; transport from empty fibers                    │
│ curation    gap signals, grouped for one proposal per gap                  │
│ ensemble    columns, positions, agreement / surprise                       │
│ discovery   functor candidates judged by meaning                           │
│ trace       telemetry → typed raster events + insights                     │
│ model       onto-models: Judge / Proposer / Critic contract; mocks here    │
│ telemetry   JSON lines · mem: heap counter + RSS                           │
└────────────────────────────────────────────────────────────────────────────┘
  onto-remote   Jev (TypeSafe) judge and critic, OpenRouter proposer (HTTP)
  onto-local    OpenJev judge on the device (llama.cpp / wllama)  docs/11
┌─────────────────────── onto-core (Rust, synchronous) ──────────────────────┐
│ category    objects + arrows, u32 ids, CSR frames; learned layer; admission │
│ parse       `.onto` modules: categories, functors, ensembles, imports      │
│ path        type-checked composition                                       │
│ equality    laws + equations as egg rewrites (Equal / Distinct / Unknown)  │
│ walk        the step, dispositions, escalation kinds                       │
│ state       state policy; `unseen` proofs                                  │
│ require     preconditions over the case, in code                           │
│ attest      attesters, Ed25519-signed observations                         │
│ laws        capability laws with proofs; quotient: bisimulation            │
│ supervise   structural proofs of a proposal                                │
│ functor     maps between categories and their checks                       │
│ ensemble    ensemble declarations; structural agreement                    │
│ discover    admissible roles, search, declarations                         │
└────────────────────────────────────────────────────────────────────────────┘
┌─────────────────────── onto-cli (the `onto` binary) ───────────────────────┐
│ check ls compose eq reach laws quotient functor discover                   │
│ walk ask run ensemble why replay raster                                    │
│ review promote curate learned migrate keygen attest                        │
│ today it also holds the "world": loading modules, layers, lenses, the      │
│ library and precedents; saving learned arrows, recalls, gaps, memory       │
└────────────────────────────────────────────────────────────────────────────┘
   web/raster: TypeScript + Apache ECharts, bundled into `onto raster`

   M4: the app API (load · decide → Outcome · actions · pending / answer ·
       storage), the CLI rebuilt on it; then PyO3 and a C ABI over it
   M5: OSIL bridge
```

`onto-core` stays synchronous and light (egg, serde_json, sha2, ed25519; serde optional)
so it can sit behind a C ABI. `onto-runtime` adds the async loop; the
model contract lives in `onto-models` and the network clients in
`onto-remote`, so an app with only a local judge ships no HTTP client
(`docs/11-platforms.md`). `onto-cli` (the `onto` binary) is **not yet** a thin
shell: it also holds the "world" (loading modules, the learned layer,
lenses, the library and precedents; saving learned arrows, recalls, gaps
and memory as files next to the `.onto`). M4 moves that into an app API
the CLI and the bindings share (§10).

## 4. The two-tier step

```
step(case, A):
  frame = arrows out of A whose `require` holds in case     (code, no model)
  if frame is empty:                escalate (none_of_these if A is closed,
                                              open_frame if A is open)
  answer = Judge(frame, A.frame.primitive, hops so far)     (one request)
    choice: one arrow or none_of_these      → follow, or escalate
    noul:   P(holds) per arrow (+ P(fork))  → none: escalate; one: follow;
            several: fork into branches if the judge says "independent
            aspects" and the branch budget allows, else follow the best and
            log the rest as alternatives
    score:  P(level)                        → follow the level's arrow
  nothing fits: none_of_these (closed: its MECE claim failed)
                or open_frame (open: an expected gap)
  low confidence                    → escalate (low_confidence)
  escalate → Proposer, shown the provisional proposals already pending at A
           → proposals (with `about`), recorded as provisional
```

Rules:

1. **"None of these" is always a candidate.** A System-1 model must be able
   to say the frame does not fit (TypeSafe guidance: include a no-match
   outcome).
2. **Proposals never enter the declared policy directly.** In the open
   world (the default) a proposal the supervisor does not reject joins
   the **learned layer** and the walk continues through it; in the closed
   world (`--closed-world`) it stays provisional. Either way the `.onto`
   file changes only by `onto promote` (a person). See rule 15.
3. **Learned structure closes gaps**, so the fast path covers more over
   time: the next walk's System 1 takes a learned arrow without asking
   System 2. Learned structure is a library, never deleted: active,
   dormant (recalled at a gap before any catalogue or model) or retired
   (a person's decision); `docs/09-learned-library.md`. Promotion moves
   an arrow into the declared file.
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
   like any other. Each branch carries its **focus** (the arrow and
   condition that spawned it), so it judges and proposes for its own aspect.
8. **Evidence before judgment.** `require` runs in code first; a missing
   field fails its clause, so an arrow never opens on absent evidence, and
   no model is asked when code has already ruled every arrow out.
9. **Open frames are judged too.** "Open" means the enumeration is known
   to be incomplete, not that no arrow can fit: an existing arrow that fits
   is followed. Open frames take a read claim like closed ones.
10. **One answer per gap.** The frame claim is released before any model
    call (MVCC, D54); proposals are keyed by gap and one is in flight per
    key (single-flight, D55): a walk meeting the same gap subscribes to
    the answer. Proposals stay visible per frame (`pending_here`), and the
    next proposer there is asked to reuse one unchanged when it fits.
    Same-named proposals group as conceptual potentialities. A walk whose
    frame grew after it was judged judges again before asking anyone
    (D63).
11. **Capabilities, not facts.** Walks carry tokens. An arrow is enabled
    when its own `require` holds on the case and its target's entry
    contract holds on the case and on the tokens the walk would hold after
    the arrow's effects (revokes, then ensures):
    enabled(a, s) = require_a(case) ∧ entry_dst(a)(case, effect_a(tokens)).
    A new walk may only start where the entry contract holds with no
    tokens, so a start in the middle of the graph cannot skip a contract.
    Blocked arrows get the disposition `blocked_by_entry`.
12. **Laws drop out.** `onto laws` explores the exact (object, tokens)
    state space and reports, per object, the tokens every walk must and
    some walk may hold on arrival, the derived laws, dead arrows, and
    whether each `via` invariant is enforced for walks (not only for the
    graph). Review notes a proposal no walk could take.
13. **Capabilities have issuance authority.** A token certifies that
    evidence was checked by an authorized transition, not that some arrow
    ran. Every token must be declared; only a declared issuer may
    `ensures` it and only a declared revoker `revokes` it; no arrow both
    issues and revokes one; declared starts must be enterable with no
    tokens. One validator (`Category::validate_capabilities`) serves every
    route in: loading, the supervisor's hypothetical extension, and
    promotion (which re-checks and then reloads). Authority is by arrow
    name and names are unique, so a proposal cannot impersonate an issuer.
14. **Laws come with proofs, in two scopes.** `onto laws` runs from the
    declared roots (guarantees of this application) or all startable
    objects (guarantees of the category itself). Every MUST law carries an
    exhaustive certificate (all arrival states enumerated, none without
    the token, the issuers and revokers actually used); `--proofs` adds a
    shortest path per arrival state, and a witness plus a counterexample
    for every MAY-but-not-MUST claim; dead arrows say why. The exploration
    is exact over |objects| × 2^|tokens| states: fine for today's graphs,
    exponential in tokens (later: bitsets, BDDs, per-capability dataflow).
15. **Open world by default; two layers.** When a frame's enumeration is
    missing (none of these, open frame, low confidence), System 2
    proposes, the supervisor reviews against the **live** graph with the
    judge as critic, and every proposal that no check fails (structural
    proofs, rule / duplicate / overlap critic checks; `unknown` allowed)
    is admitted as a **learned** arrow; the walk re-judges the same frame
    with it. The *declared* layer (the `.onto` file: capabilities, entry
    contracts, invariants, attesters, closure claims, roots) is policy and
    is never written by a run. The *learned* layer is appended to
    `<stem>.learned.jsonl` and replayed on every load through the same
    structural proofs, against the current policy: a learned arrow that
    no longer passes under a changed policy is not loaded; learned
    arrows can never change policy. Learned arrows carry no capability effects they could
    not already have (authority is by declared issuer name), inherit every
    entry contract, and obey **progress**: a learned arrow may not close a
    cycle. Each walk may extend at most `max_expansions` frames (default
    3). Records mark learned candidates and steps; answers reached through
    them say so. **Admission regimes (D51):** per frame, `admission X:
    open_world | assured | sealed;` (default `admission: …;`). Open
    world learns what no hard check refutes, semantic unknowns included;
    assured learns only what every check passed and holds the undecided
    for a person; sealed learns nothing. Hard failures are rejected in
    every regime; a closure challenge is not an unknown. A run can
    tighten (`--loop assured`, `--closed-world`), never loosen. **Sealed regions:** `sealed: A, B;` or `world: closed;`
    with `learnable: A, B;` (policy) mark where the open world may grow;
    no learned arrow leaves or enters a sealed object, so sealed objects
    are reached only by declared arrows. Objects the open world creates
    are learnable. A sealed frame escalates to a person in any mode.
16. **What a model sees is policy.** A System-1 call has no memory: its
    `state` is its whole context. `state { … }` (default) and `state A, B
    { … }` (override) declare it: `goal`, `case` or `case: a.b, …`,
    `observed` (the attested view, verified only), `history` / `history:
    last N` (earlier judgments, marked inferred), `focus`, `tokens`. The
    state is sectioned by epistemic status: `asserted` (case facts),
    `observed` (signed), `inferred` (earlier model judgments). The
    proposer sees the same state as the judge at that frame; the critic
    sees no case at all. `invariant unseen: case.x` is proved on load and
    for every proposal. Records keep the exact state sent (`seen`).
    With no `state` declared, frames see what they always did.
    `memory: similar N` adds precedents: earlier decisions at the same
    frame for other cases, **projected onto the frame's current
    declaration** (so `unseen` covers memory), chosen by lexical
    similarity, loaded from `<stem>.memory.jsonl`. `onto replay` re-asks
    the judge with a recorded `seen` state changed (`--drop`, `--set`)
    next to an unchanged baseline, under the same confidence gate.
17. **An escalation is typed, and only a structure gap asks a model.**
    Every escalation is classified (`EscalationKind`, D53):
    - a **structure gap** where structure may be learned: the library
      (dormant arrows from this frame), then transport (a functor's empty
      fibers), then the proposer, on the case's path;
    - an **evidence gap** (an option held back by a missing attestation,
      precondition or entry contract): no model, the record names what is
      missing;
    - a **policy stop** (a sealed frame; a structure gap in a closed-world
      run): a gap signal for curation, the case does not wait;
    - a **budget stop**.
    `docs/08-call-economy.md` measures it; `docs/09-learned-library.md`
    the library.

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
claim footprint(A) (read)       every frame is judged, open or closed (rule 9)
  Judge (Jev)                   → follow or fork; release; next step
  else classify the escalation  (rule 17)
    evidence / policy / budget  → record, gap signal if curation; stop
    structure gap               → release the claim (MVCC)
      frame grew since judged?  → judge again
      library recall            → admit, judge again
      transport                 → review, admit, judge again
      proposer (single-flight)  → review under the frame's admission regime:
                                   admit (learned) · hold (person) · refuse
record the frame visit (disposition record, gap key, what was seen)
```

### 5.1 Claims and policies

A walk holds at most one claim, taken all-or-nothing over the footprint,
so claims cannot deadlock. Whether two intersecting claims wait:

| policy | waits when | use |
|---|---|---|
| `exclusive` (audit mode) | footprints share any node | decision frames that could converge are decided one after another |
| `shared` (default, D57) | same frame, and one claim is a write | readers of an unchanged frame run together; only frame edits serialize |

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
| `run.start` | category, ensemble, walks, judge, proposer, policy, speculate, threshold, max_branches, max_fork_depth |
| `walk.start` / `walk.end` | walk, parent, category, case, from, goal / path, steps, elapsed_ms |
| `judge.call` | walk, at, primitive, questions, latency_ms, top_p, holds, fork_p, confidence, input_tokens, output_tokens, attempts, ok (error on failure) |
| `fork` | walk, at, fork_p, branches, spawned |
| `proposer.call` | walk, at, speculative, latency_ms, proposals, pending_here, reused, input_tokens, output_tokens, attempts, ok |
| `proposer.discarded` | walk, at (speculative call aborted) |
| `step` / `escalate` | walk, from, arrow, to, decided_by, p, confidence, alternatives / walk, at, reason |
| `potentiality` | kind (node, conceptual, alternative), resolution (waited, coexisted, not_followed), mode, walk, with, at, nodes (comma-joined), wait_ms |
| `visit` | walk, at, record, claim_wait_ms |
| `gap` / `gap.subscribe` / `gap.stale` | walk, at, record, kind, reason, gap key, typed route, route / the gap subscribed to / a graph that grew (grown arrows) |
| `expansion` / `learned` / `held` / `refused` | walk, at, record, source (library, transport, proposer, shared), counts / the arrow admitted, held or refused and why |
| `recalled` / `restored` | walk, record, arrow, from, to: a dormant arrow recalled; active structure restored with it |
| `join` | walk, at, record, policy, role, into, merged, wait_ms |
| `ensemble.start` / `.position` / `.surprise` / `.confirm` / `.outcome` | ensemble, shared, job, column, walk, object, shared position / the conflicting positions / status, agreed, route |
| `mem.sample` | rss_bytes, heap_bytes, heap_peak_bytes (every 250 ms) |
| `run.end` | wall_ms, model_ms_sum, judge_calls, judge_questions, proposer_calls, forks, branches, tokens, potentialities, peak_rss_bytes, heap_peak_bytes |

Memory: heap bytes are exact, from a counting global allocator the binary
installs (`mem::CountingAlloc`); RSS comes from the OS (`memory-stats`).
`--report` writes the whole run (walks, steps, proposals, potentialities,
memory) as one JSON document.

### 5.4 Disposition records

Every frame visit produces a `FrameRecord`: the claim (mode, wait), the
judge call (model, latency, questions, confidence, none_of_these, fork p,
tokens), **every** outgoing arrow with its judgment, disposition and a
deterministic reason, the outcome (followed · forked · escalated ·
failed) and any provisional proposals. Records chain through `after`: the
previous visit of the same walk, or, for a branch's first visit, the fork
that spawned it. A run's records are the disposition graph.

- `onto run … --dispositions run.dispositions.jsonl` writes them (one
  record per line), separately from telemetry.
- `onto why run.dispositions.jsonl [--walk N]` prints them as
  explanations; `--walk` follows a branch's lineage back through its fork.
- `onto ask … --why` shows them to the beneficiary; options closed by
  `require` are named in the answer itself.

### 5.5 The supervisor (M2)

Proposals are reviewed before anything may enter the graph:

```
proposal ─▶ well_formed   names, types, target new or existing         (code)
         ─▶ invariants    via / never, on the graph WITH the proposal  (code: proof,
                          failure carries a counter-path)                or witness)
         ─▶ rules         natural-language invariants                   (critic: Noul)
         ─▶ duplicate     same kind of case as a sibling arrow, committed
                          or proposed earlier in the batch              (critic: Noul)
         ─▶ overlap       could one case fit both (choice frames only:
                          would break the MECE claim)                   (critic: Noul)
         ─▶ admit · reject · unknown
```

- Structural checks run first; a proven reject asks no model. A reused
  arrow name does not hide a bypass: invariants are still evaluated under
  a temporary name and reported.
- All semantic checks for one proposal go to the critic (Jev) in one
  request. Fail at P ≥ 0.7, pass at P ≤ 0.3, unknown between: the
  supervisor does not guess.
- The base graph must satisfy its own `via`/`never` invariants to load.
- The frame's admission regime decides what a review admits (rule 15):
  under `assured` an undecided semantic check **holds** the proposal for
  a person instead of admitting it.
- `onto review FILE DISPOSITIONS --out reviews.jsonl` reviews every
  provisional proposal (identical ones merged, sources kept).
- `onto promote FILE reviews.jsonl rN` is the human step. Only `admit`
  promotes; `unknown` needs `--override-unknown`; `reject` never does. The
  structural checks are re-proved against the current file (a stale
  review is refused), the arrow (and a new, open object) is written into
  the `.onto` file with a provenance comment (date, review, verdict,
  override, sources), and the edit is rolled back if the file would not
  load. Git is the delta log.

### 5.6 Measured (2026-09-23, 6 walks, `examples/triage.jobs`, release build; before MVCC and the `shared` default)

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
    g: B -> C require case.ok == true ensures T;   # checked fact grants capability T
    w: C -> B revokes T;                       # capability withdrawn
    entry D: needs T require case.flag == true;    # entry contract, inherited by every arrow into D
    capability T { issuers: g; revokers: w; }  # only g may issue T, only w revoke it
    start: A;                                  # declared application root
    join D: all;                               # branches of a fork recombine here (all | race | gate authority g export T)
    attester Ops { key: ed25519:<base64>; observes: x.done; }   # trusted source (policy)
    h: C -> D "…" attested x.done == true;     # opens only on a verified, signed observation
    invariant via: A -> D through B | C;       # every path A→D passes B or C (proved)
    invariant never: A -> Z;                   # no path A→Z, even to a future Z (proved)
    invariant rule "no arrow may …";           # judged by the critic
    invariant unseen: case.person;             # no model ever sees this field (proved)

    frame R: noul parallel "Which plans?";     # every holding arrow runs as a concurrent alternative
    frame S: split;                            # no judgment: every eligible arrow runs
    frame T: choice grouped by ByTeam "…";     # choose a group (a functor's image) first, then within it

    state { goal; case: a.b, c; }              # what every model is shown (default)
    state A, B { case: a; observed; history: last 3; memory: similar 3; focus; tokens; }

    admission: open_world;                     # default regime: open_world | assured | sealed
    admission A: assured;                      # per frame
    sealed: B, C;                              # or: world: closed; learnable: A;
}
```

A file is a **module**: categories, functors, ensembles and imports.

```
import "../../standards/change-control.onto";  # relative to this file; a standard is loaded once

functor F: Name -> Other {
    objects: A -> X, B -> Y;                   # partial: unmapped objects are outside the domain
    f: g;  h: k.g;  w: id;                     # an arrow maps to a path (k after g) or an identity
    capabilities: T -> U;                      # capability names across the two
    require: authority, contracts, invariants, cover;   # load errors when not met (else reported)
    by name;                                   # a version: unmapped names map to the same name
    transport;                                 # empty fibers are proposed at structure gaps
}

ensemble E {
    shared: IncidentState;                     # the category positions are compared in
    column Metrics: MetricsView from Signal;   # a category, its functor into shared, where it starts
    column Logs: LogsView from Entry;
    consensus: all;                            # all | quorum N
}
```

`FILE#Name` names one category, functor or ensemble of a module on the
command line.

Statements end with `;` (outside strings and JSON), `#` starts a comment
(outside strings), objects are declared before use. After `Src -> Dst` an
arrow takes, in any order, `level N` and an instruction, then optionally
`require EXPR` last. `require`: dot paths into the case JSON, `== != < <=
> >=` against JSON literals, joined by `and`; a bare path must be truthy.
Jobs are `Start: text` or `Start: {"goal": "...", ...facts}`. `o`, `id` and `none_of_these` are reserved names. Object and arrow names share one
namespace.

## 8. Storage

The hot operation is a step: read one frame, choose, compose. Frames are
stored CSR (one contiguous slice per object), and the category lives in
memory: every demo loads in milliseconds, so the planned rkyv snapshot
and mmap are not built (not needed at these sizes). A category's
**snapshot** is the SHA-256 of its source; records, reviews and gap keys
carry it, and a stale review is refused.

Files, next to `<stem>.onto` (or `<stem>.<Name>.*` for `FILE#Name`):

| file | holds | written by |
|---|---|---|
| `<stem>.onto` | declared policy | people (`onto promote` writes a reviewed arrow with provenance); **git is the delta log** |
| `.learned.jsonl` | the learned library: one arrow per line, its checks, provenance, `state` and `note` | runs (append), recalls and `onto learned` (state changes) |
| `.memory.jsonl` | precedents for frames that declare `memory` | runs |
| `.gaps.jsonl` | gap signals for curation | runs |
| `--dispositions`, `--telemetry`, `--report` | frame records, events, the whole run | runs, on request |
| reviews, curated proposals | review verdicts; one proposal per gap group | `onto review`, `onto curate` |

Sibling files suit a CLI and a repository. An app needs a storage
interface instead (a database, an object store): part of the M4 app API.
Dispositions are written at the end of a run (a crash loses them);
streaming them is open (§11).

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
| D8 | *(superseded by D57)* default policy `exclusive`; `shared` opt-in | follows the stated method (intersecting frames wait); `shared` measured faster |
| D9 | proposer default `~openai/gpt-luna-latest`, `reasoning.effort = low` | reasoning models otherwise spend the token budget and return no content |
| D10 | telemetry as JSON lines via `tracing`; heap via counting allocator | greppable, `jq`-able, exact heap numbers |
| D11 | frames declare a primitive (choice/noul/score); arrows carry instructions (text or JSON) | meaning is not tied to one question type; Jev evaluates all three natively |
| D12 | fork vs follow-best decided per frame by a Noul over state and hops, guarded by a code budget | the case decides, within limits code enforces |
| D13 | `require` preconditions evaluated in code over case JSON | known rules stay deterministic; evidence gates judgment |
| D14 | System-1 interface renamed Chooser → Judge; telemetry `chooser.call` → `judge.call` | it no longer only chooses |
| D15 | open frames are judged; `open_frame` now means "nothing fit in a frame known to be incomplete" | an arrow that fits should be followed; the two gap kinds stay distinguishable |
| D16 | proposers see the provisional proposals pending at their frame and reuse fitting ones | turns waiting on a frame into deduplication; fixes name-based grouping |
| D19 | invariants: structural (`via`, `never`) are proved with counter-paths; `rule` is judged | proofs where possible, judgment where not; mirrors `require` vs instructions |
| D20 | promotion is a human step that edits the `.onto` file; git is the delta log | one source of truth; reviewable diffs; unsafe proposals were seen live |
| D21 | semantic checks have an unknown band (0.3 < P < 0.7) that blocks automatic admission | the supervisor must not guess |
| D22 | the rkyv/mmap snapshot moves out of M2 | performance, not governance |
| D23 | capability tokens with `ensures`/`revokes` on arrows and `entry … needs` on objects; facts stay in the case | entry contracts need something to make them true; walking an arrow must not make a real-world fact true |
| D24 | starting at an object is entering it | otherwise a mid-graph start skips contracts (found while deriving laws) |
| D25 | derived laws by exact exploration of (object, tokens) states, case preconditions assumed satisfiable | exact for tokens, sound over-approximation for cases; small state spaces |
| D27 | capabilities are declared with issuers and revokers; strict: every token must be declared | without issuance authority an arrow can counterfeit the evidence a contract asks for (`fake_basis … ensures LegalBasis`) |
| D28 | one capability validator for loading, review and promotion | a governance rule must not behave differently by entry route |
| D29 | `start:` roots; laws in two scopes (declared roots / all startable) | application guarantees and category guarantees are different claims |
| D30 | MUST laws carry exhaustive certificates, MAY laws witnesses, failures counterexamples | a single path is not a proof of "every walk" |
| D31 | structured joins only (siblings of one fork); all / race / gate; intersection by default, gate exports an allowlist | known sibling sets avoid OR-join semantics; least privilege; authority transfer is explicit |
| D32 | a walk waits only on running siblings | deadlock freedom without a global scheduler |
| D33 | attested observations: declared attesters with Ed25519 keys, strict verification over a domain-separated canonical message, field authorization, case binding | a real-world claim must be authenticated, not asserted; replay across cases is refused |
| D34 | completion is presented only on attested evidence: arrows can require it, joins and answers are labelled attested or not | a walk reaching an object is evidence about the walk, not the world |
| D35 | `noul parallel` frames pursue every holding arrow without a fork question | racing alternatives (mitigation plans) are neither independent aspects nor competing readings; the author declares them |
| D37 | open world is the default: supervisor-admitted proposals become learned arrows and the walk continues; `--closed-world` restores stop-and-promote | escalating to a person at every missing enumeration made the index unable to grow by itself; safety lives in the proofs, not in stopping |
| D38 | two layers: declared policy (`.onto`, human) and learned structure (`.learned.jsonl`, replayed through the proofs on every load) | exploration must not require editing policy; a policy change must be able to retire what was learned under the old one |
| D39 | learned arrows may not close a cycle (progress) | live, a proposer learned `Shipping -> Ticket` despite being told not to, and every later walk looped until the step limit; a prompt is not a guarantee |
| D40 | the proposer is shown the graph's outcomes and the declared arrows that finish into them | live, learned branches kept adding intermediate objects and never reached `Resolved` |
| D41 | model input (`state`) is declared per frame, sectioned asserted / observed / inferred, and recorded | data minimization must be a checkable policy, and models must know which facts are evidence and which are guesses |
| D42 | `unseen` proves field visibility, not text content | free text (`goal`, descriptions) cannot be proved clean; `onto laws` names every frame that sees free text instead |
| D43 | sealed regions (`sealed:`, `world: closed` + `learnable:`) bound the open world per object | a global open/closed switch was too coarse: in consent-enforcement the open world learned at the consent check; regulated regions need structure only people declare, while new kinds of request may still be learned |
| D44 | memory is precedents projected onto the current state declaration, only from loaded runs, never the case itself | memory must not become a side channel around `unseen`, and must be reproducible within a run |
| D45 | counterfactual replay always runs an unchanged baseline beside the change | live, the same state gave confidence 0.47 in the walk and 0.29 in replay: without a baseline, model instability would read as an effect |
| D46 | functors are partial (defined on the mapped objects) and policy is **reflected** onto the source, on objects | a standard describes part of an organization; a required step must be a visible step of the source, not hidden inside one arrow |
| D47 | precedents do not migrate across a change to their frame's options | a precedent answers a question; when the options change it answers a different one (live: v1's wrong carer decision) |
| D48 | a grouped frame backs off to the whole frame when the fiber holds nothing | live, a confident coarse step sent an accessibility ticket to the wrong team and the fiber could not recover |
| D49 | transport runs before the LLM proposer; its proposals carry their functor and target arrow | known structure should be reused before new structure is invented, and learned structure should say where it came from |
| D50 | overlap/duplicate between options of a closed target frame are settled by that frame, not re-judged | the closed claim is policy people declared; live, a model re-litigating it blocked the catalogue's options |
| D51 | two evolutionary regimes over one runtime, chosen per frame: open-world learning (semantic unknowns may enter the learned layer as hypotheses with provenance) and assured evolution (unknowns wait for a person); closure, admission and layer are separate concepts | one trust regime cannot serve both a support taxonomy and a consent policy; applications must state how they live with the unknown |
| D52 | the raster renders a typed trace projection (`trace::RasterEvent`), not telemetry; v1 renderer: Apache ECharts (Canvas) in TypeScript, embedded in one HTML page; deck.gl only if measured scale requires it | the data model must outlive the renderer; a self-contained page needs no server; Canvas handles thousands of marks |
| D53 | escalations are typed (structure gap, evidence gap, policy stop, budget stop); the proposer serves structure gaps only | a walk that cannot continue does not always need a new arrow: live, 42–84% of proposer time went where no structure could be learned |
| D54 | no frame claim is held during a model call (MVCC); admission re-validates under the graph's write lock | a proposer holding a claim made healthy cases wait for an LLM (07 §3, §6); the lock goes, the checks stay |
| D55 | single-flight per gap key: walks meeting a gap in flight subscribe to its answer | the same gap need not be answered twice; rule 10's reuse without making readers wait |
| D56 | stops only a person can act on become gap signals for curation (`onto curate`: one proposal per gap, representative cases as the state policy showed them) | a case should not wait for proposals it cannot use; repeated gaps are better answered once, from several cases |
| D57 | default policy `shared`; `exclusive` stays as an audit mode | with MVCC (D54) waiting is not needed for correctness; intersections are still logged as potentialities; measured: exclusive made an entry frame a queue of 17 and a run 3.6× slower (`docs/07` §2) |
| D58 | ensembles: columns agree when one position reaches the other in the shared category; otherwise a surprise | contradiction is defined by structure people declared, not judged by a model; one perspective being further along is not a disagreement |
| D59 | a surprise is routed by the shared positions' admission: `assured`/`sealed` → a person; `open_world` → a `contradiction` gap signal keyed by where the positions diverge and which positions conflict | the same blind spot across cases is one gap; guarded decisions never learn from a disagreement without a person |
| D60 | consensus counts only columns that concluded something; a quorum settles the position but a dissent is still a contradiction (a person under guarded admission, a gap signal in open world); under guarded admission an incomplete or undecided case goes to a person | live, a start position (reaches everything) read as agreement while the social column had said nothing; with quorum 2, "medically fit" and "wants to go home" outvoted "no safe home": the number of perspectives must not silence the one that sees the risk |
| D61 | learned structure is a library, never deleted: active (in the graph), dormant (out of it, recallable), retired (a person's); the state lives in the learned layer | an arrow's presence shapes every judgment at its frame, taken or not; removing it must be reversible and measured |
| D62 | at an open-world structure gap: library → catalogue (transport) → model; a recall reactivates the arrow and restores active structure that hung from it | the cheapest answer first; a recall is evidence the structure is needed; nothing is admitted that the open world could not already learn, and the proofs run again |
| D63 | a walk whose frame grew after it was judged judges again before recall, transport or a model; checked after its own recall attempt | live, four tickets judged in the same instant as a recall met their gaps 10 ms later on the old graph: one proposer call (4.2 s) for an answer that existed, four tickets stopped |
| D64 | dormancy and retirement are a person's act; `--usage` and replay only recommend, and a dormant arrow is audited with `replay --with-arrow` | live, "never taken" meant no one bereaved had applied; with the arrow dormant, bereaved applicants went confidently to `none_apply` and `is_carer` (no gap, no recall); with it present, both took it at 1.00 |
| D65 | functor discovery: structure decides what is possible (roles; every arrow a path), evidence ranks it (meaning: a judge chooses among admissible targets or none; behaviour: co-visits of the same cases); a model proposes only what these leave open | live, structure alone mapped 2 of 12 objects, behaviour 9, meaning 12; structure rescued a weak judgment (0.28) |
| D66 | a discovered functor is a proposal with provenance, adopted by a person; authority and contract violations are reported, not pruned | a functor is policy (transport, views, standards, ensembles); the true map of the merger violates authority, and that is the finding |
| D67 | behaviour keeps testing an adopted functor: pairs the same cases rarely share are flagged | a functor can be sound while the organisations handle those cases differently (live: how-to tickets, closing steps) |
| D68 | the model contract (`Judge`, `Proposer`, `Critic`, requests, errors, type-erased adapters) is its own crate, `onto-models`, with no engine and no network | local judges, remote clients and every binding implement one contract without depending on the engine or on reqwest (`docs/11`) |
| D69 | network providers move to `onto-remote`; `onto-runtime` no longer depends on reqwest | an on-device app with local models ships no HTTP client and no keys; parallel API calling is a module an app adds |
| D70 | the runtime asks an `rt` module to spawn, sleep, read the clock and collect tasks: tokio natively, the JavaScript event loop on `wasm32` (by target); `tokio::sync` stays | the engine must run in a browser; on one thread walks still interleave while a model call is pending |
| D71 | local judges use the OpenJev method (option logits in one forward pass) over GGUF through llama.cpp, natively and as wllama in browsers; one model loaded, parallelism by shared prefixes; thresholds re-measured per model | zero token cost and private; the same model file on every device makes results comparable; copies of a model contend for one GPU; Jev's calibration does not transfer |
| D72 | Swift and Kotlin through UniFFI (`onto-ffi`), JavaScript through wasm-bindgen (`onto-wasm`); both only convert, as `onto-py` does | one Rust source of truth for every platform (`docs/10` rule 1) |
| D36 | `split` frames: an AND-split with no judgment; over budget they escalate (`split_over_budget`) | live, a judged verification frame dropped a mandatory check and an all-join completed without it |
| D26 | a proposal into a closed frame is a closure challenge (`unknown`), not a falsification | the proposal may be nonsense or a duplicate; only a validated novel arrow revises the claim |
| D18 | after a fork, every branch (including the walk that continues) carries its focus: the spawning arrow and its condition; judges and proposers are told to handle that aspect only, and records store it | branches otherwise inherit the whole case and propose for each other's aspects (seen live) |
| D17 | disposition records are a first-class artifact (own file, own schema), with deterministic reasons and causal `after` links | provenance must not depend on reconstructing telemetry; reasons must be reproducible, not generated |

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
- **M2 (done):** invariants (via with alternatives, never, rule), the
  supervisor (structural proofs + one-request semantic checks), `onto
  review`, `onto promote` with human approval; git as the delta log.
- **Capabilities (done):** tokens, entry contracts, starting-is-entering,
  `onto laws` (derived laws, dead arrows, walk-level invariant checks),
  closure challenges and contract notes in review.
- **Capability authority (done):** declarations, strict validation on
  every route, declared roots, proof-producing laws; the `fake_basis`
  minting attack fails on load, in review and at promotion.
- **Frozen (authority milestone):** trust model, proof scope and snapshot
  atomicity documented in `docs/04-trust-model.md`: policy declarations
  are the root of trust; policy changes are a privileged operation (not
  yet enforced); known snapshot gaps listed with fixes.
- **Joins (done):** all, race, gate; deadlock-free waiting; merge nodes
  in the disposition graph; incident-graph mitigates only when every
  forked aspect is mitigated.
- **Trust-model gaps (done):** snapshots, stale-review refusal, atomic
  promotion. **Attested observations (done):** attesters, `attested`
  preconditions, `onto keygen` / `onto attest`, attested-completion labels.
- **Open question, owned by legal review (not an engineering decision):**
  with SecOps' signed `key.old_rejected`, the only arrow out of Rotate is
  eligible, yet the judge was unsure and escalated. Should a closed frame
  whose single eligible arrow is opened by attested evidence be taken
  without asking the judge ("decided by evidence")? It would let signed
  evidence outrank a model's hesitation. Until legal decides, the current
  behaviour stands: the judge decides, and an unsure judge escalates to a
  person.
- **Behavioural quotient (done):** `onto quotient` reports duplicates,
  name-only identities, description-only identities (the asymmetry
  check) and redundant arrows.
- **Open world (done):** default open world, learned layer, progress
  rule, `--closed-world`, `onto learned`; live on support-commons, a
  delivery ticket learned `Ticket -> Delivery -> Resolved`, and the next
  session's ticket reused it and grew the Delivery frame.
- **State policy (done):** `state` declarations, sectioned model input,
  `invariant unseen`, `seen` in records, `onto laws` visibility section;
  consent-enforcement shows no person data reaching any model, proved.
- **Sealed regions (done):** `sealed:`, `learnable:`, `world:`; `onto
  laws` reports them; consent-enforcement learns only new kinds of use.
- **Memory and replay (done):** `memory: similar N`, `onto replay`;
  benefits-assembly: applicant record unseen (proved), a MECE gap
  (carers) found, precedents shown to change a decision.
- **secure-infrastructure-change (done):** judgment, evidence and
  authority as three layers; refusals recorded; the proposer is told
  what is sealed; `onto laws` shows which frames may reach a proposer.
- **Functors, phase 1 (done):** modules and imports, `functor` blocks
  (partial, by name), well-definedness and equations proved, authority /
  contracts / invariants reflected, coverage, evidence backing; `onto
  functor`, `run --view`, `onto migrate`; `standards/`. See
  `docs/05-functors.md`.
- **Functors, phase 2 (done):** `grouped by` with fallback and backoff;
  transport from empty fibers before the LLM, settled-by-structure
  sibling checks, transported structure restored across runs.
- **Admission regimes (done):** `admission` per frame (open_world,
  assured, sealed), `held` proposals, `--loop assured`; README states the
  shared core and both loops; `docs/06-spaces.md` names the four spaces.
- **Call economy (done):** typed escalations, gap keys, MVCC, single-flight,
  gap signals and `onto curate`; measured before and after each step
  (`docs/08-call-economy.md`).
- **Ensembles (A1 done):** `ensemble` blocks, `onto ensemble`, positions
  through functors, structural agreement, surprise routed by admission,
  contradiction gap signals, the shared band in the raster
  (`docs/05-functors.md` §6, `docs/07-raster-findings.md` §8).
- **Plan (agreed 2026-09-24), each step closed by its demo test and a
  re-run of the earlier scenarios:**

  | step | builds | demo · beneficiary | tests |
  |---|---|---|---|
  | A1 ✓ | M3 phase 3: ensembles (columns, shared category, prediction, surprise) | incident-response ensemble (metrics, logs, complaints) · customers during an outage | agreement; a monitoring blind spot; competing causes; raster item 8 |
  | A2 ✓ | consensus policies; surprise routed by admission | hospital-discharge ensemble (clinical, social) · a patient leaving hospital | a surprise stops an unsafe discharge (assured → a person) |
  | B ✓ | learned-structure library: presence effect (`replay --without-arrow`), active / dormant / retired, recall before catalogue and LLM | incident-response (unused learned arrows) · support-commons (parcel stream) | how an arrow's presence shifts judgments; recall with no model call |
  | C ✓ | M3 phase 4: functor discovery | support-commons, two support organisations merging · their customers | discovered vs hand-written map; wrong candidates caught by the functor checks |
  | D | M4: onto as an embeddable engine with unopinionated Python bindings (primitives, protocols, values; basic and advanced levels), then a C ABI; `docs/10-bindings.md` | consent-enforcement · developers | a developer given only the docs builds the consent app in Python, basic and customized: time to a first correct decision, and whether they had to read Rust |
  | F | onto on every platform with local models: `onto-models` / `onto-remote` split, a wasm-capable runtime, the OpenJev local judge, UniFFI and wasm bindings; `docs/11-platforms.md` | support-commons, incident-response, consent, hospital-discharge / benefits as on-device apps · people whose cases must not leave the device | accuracy and p50/p95 per device (M4 Mac, iPhone 15 Pro – 18 Pro) with no network, beside Jev |
  | E | M5: OSIL bridge (functor reports as preservation contracts) | secure-infrastructure-change, or OSIL's own repository governance · to be grounded in OSIL's docs first | an onto functor report and an OSIL preservation claim say the same thing |

- **Consensus policies (A2 done):** only columns that concluded count;
  a quorum's dissent and an incomplete case under guarded admission go
  to a person (D60); hospital-discharge ensembles, live.
- **Learned-structure library (B done):** active / dormant / retired in
  the learned layer; `onto learned` state changes and `--usage`
  recommendations; `onto replay --without-arrow` / `--with-arrow`; recall
  before transport and the proposer; stale judgments judged again
  (D61–D64, `docs/09-learned-library.md`). The incident-response layer no
  longer learns since typed escalations (its gaps are evidence gaps), so
  the presence test ran on benefits-assembly.
- **Functor discovery (C done):** `onto discover`: structure prunes,
  meaning (Jev) and behaviour (co-visits of shared cases) rank; the
  discovered map is a proposal checked like a hand-written one;
  adopted with `transport` it completes enumerations without a model
  (D65–D67, `docs/05-functors.md` §7).
- **Audit before M4 (2026-09-24).** M1, M1.5, M2, M3 (phases 1–4) and
  every item above are built and tested. Deferred by design: column
  switch (phase 3b), natural transformations, legal mapping (legal
  review), KOfN / Accumulate joins, geometry. Open engineering items:
  streaming dispositions; joins in `onto laws`; name-based conceptual
  intersection; policy change as a privileged operation (documented in
  `docs/04`, not enforced). What an app developer needs and does not
  have yet, found by walking the developer path:
  1. **resume**: a walk stopped for a person cannot continue with the
     person's answer (review and promote change the category, not the
     case);
  2. **a typed outcome**: a walk returns a path string and step records,
     not `{status: done | needs_person | needs_evidence | stopped, at,
     path, confidence, why}`;
  3. **the world is in the CLI**: loading layers, lenses, the library and
     precedents, and saving learned arrows, recalls, gaps and memory live
     in `onto-cli` (about 1,150 lines), so a binding over `Engine` would
     copy them;
  4. **actions**: arrows decide, nothing does (by design at the engine
     level: acting is the caller's; see the application library after
     M4);
  5. **entry cost**: a `.onto` file, JSON cases, two keys, 24 `run`
     flags and 24 `Config` fields before a first decision; a prompt takes
     five minutes. Onto's advantages (proved guarantees, `why`, learned
     gaps, small calls) come later, so the entry point should be a
     prompt: `onto draft` turns a description into a checked category to
     edit.
- **M4 (next, D):** onto as an **embeddable engine** with native
  Python bindings: mechanism, not policy (`docs/10-bindings.md`). The
  bindings expose primitives, protocols (a Python object can be the
  judge, proposer or critic) and state as values, at a basic and an
  advanced level; they prescribe no way of building an app. First the
  engine work (type-erased models, the world as values out of the CLI,
  events and records as a stream, resume and step), then `onto-py`
  (PyO3, maturin, abi3), then a C ABI if a consumer appears. `onto
  draft` (prompt → category + proofs) is a later, separate tool.
- **Research track (after the M4 engine work): calibrated switches**
  (`docs/12-calibrated-switches.md`): a self-building intent graph of
  calibrated binary switches, checked by loops that must agree
  (resonance), corrected by people where they are unsure; each loop run
  as an experiment. Realized by Jev, or by onto's own CPU learner,
  declared first as an OSIL contract (`contracts/calibrated-switch.osil`).
  Engine needs: labelled record export, calibration metrics and maps per
  switch, a resonance checker, the source of resume answers, a synthetic
  case generator.
- **After M4, separate:** an application library on the bindings, with
  opinions: connection mechanisms, self-expansion limits and automatic
  admission per part, which parts bring in a person, loop architectures
  and decision processes, and a developer experience where writing an
  onto application by hand is easier than writing prompts.
- **M5 (E):** OSIL bridge: onto categories as OSIL category-level
  requirements, functor reports as preservation contracts; grounded in
  `~/oaas` first.
- **Later:** raster–map–state linking; live raster; model-scoped
  capabilities for learned conceptual spaces; behavioural difference in
  review; column switch; natural transformations; geometry (research).

## 11. Open questions

- **Labelled demo cases reach the model.** `support-commons/large.onto`
  declares no `state`, so a case's whole JSON, its `expected` label
  included, is shown to the judge; accuracy measured on `large.jobs` is
  optimistic unless the label is removed (the appstudio bench does) or
  the demo declares `state { goal; }` / `invariant unseen: case.expected`.
  The reported Jev run (22 tickets, 1.4 s) was made the same way.

- **Resume (M4).** How an answer from outside (a person, another system)
  re-enters a stopped walk: the same record or a new one with an `after`
  link; the answer as an asserted or attested fact; what the record says
  about where it came from.
- **Streaming records.** Dispositions are written at the end of a run; a
  crash loses them. Streaming each record as it is made would make the
  artifact durable (and a live raster possible).
- **Privileged policy changes.** `docs/04` makes a policy change a
  governance operation; nothing enforces it yet (who may run `promote`,
  who may edit the `.onto`).
- **Fairness under `shared`** (now the default): a steady stream of
  readers can delay a writer at the same frame. No starvation seen; a
  writer-preference queue is the fix if it appears.
- **Conceptual intersection is name-based.** A Jev Noul ("do these two
  proposals denote the same concept?") would catch synonyms (`Refund` vs
  `ChargeDispute`), in single-flight grouping too.
- **Semantic checks are conservative** (live consent review: 0 admit, 5
  reject, 2 unknown; overlap judgments in 0.4–0.6). Admission regimes
  decide what an unknown does; the overlap question still needs better
  wording or calibration on labelled pairs.
- **Dormancy needs a case mix.** "Never taken" measures the runs, not the
  need (`docs/09` §3); auditing dormant arrows (`replay --with-arrow` on
  new records) is manual.
- **Wide frames.** Jev Choice holds at most 255 options; `grouped by`
  (hierarchical choice through a functor) is the answer where a grouping
  exists.
- How frames should treat arrows **derived** by an equation (should a
  defined composite appear as a choice?).
- What the pairwise **MECE verifier** asks the System-1 model, exactly.
- **Decided by evidence** (owned by legal review, §10): may a closed frame
  whose only eligible arrow is opened by attested evidence be taken
  without the judge?

Resolved since draft 7: invariants for verification (`via`, `never`,
`rule`, `unseen`); proposers reusing arrow names (the supervisor reports
what such a proposal would do); legal bases as evidence (`attested`,
`require`).
