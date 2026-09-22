# onto — Architecture

Status: **draft 0 — 2026-09-23.** Single source of truth for how onto is
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

## 3. Layers

```
        Jev / OpenJev  (System 1, RLCD-calibrated)      LLM (System 2)
                │ Chooser: p(arrow | state) + none_of_these    │ Proposer
                ▼                                               ▼
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

`onto-cli` (the `onto` binary) is a thin shell over `onto-core`.

## 4. The two-tier step

```
step(state, A):
  frame = out(A)
  if A is closed:
      d = Chooser(state, frame ∪ {none_of_these})
      if d.top is an arrow and p ≥ threshold:  follow it          (fast path)
      else: escalate (NoneOfThese | LowConfidence)
  else: escalate (OpenFrame)
  escalate → Proposer(state) → proposals, recorded as provisional
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

Rationale. Following Corballis (*The Recursive Mind*, 2011), recursion is
treated as a separable capability layered on a non-recursive base: the
graph routes; the language model generates nested structure only when the
graph runs out. Everett's claim that Pirahã lacks recursion is disputed
(Nevins, Pesetsky & Rodrigues 2009); the design depends only on recursion
being separable, not on that claim.

## 5. Path equality

Paths become egg terms: `g.f` is `(o g f)`, identities are `(id A)`.
Rewrites: associativity (both directions), left/right identity, and every
declared equation (both directions). Only well-typed terms enter the
e-graph, so the identity rewrites need no side conditions.

The word problem for finitely presented categories is undecidable, so
`Equality::check` returns `Equal`, `Distinct` (saturated without merging),
or `Unknown` (limits hit). It never guesses.

## 6. The `.onto` format

```
category Name {
    objects: A, B, C;
    f: A -> B;
    g: B -> C;
    h: A -> C;
    closed: A;          # A's frame is MECE
    g.f = h;            # path equation
    inv.f = id(A);      # identities
}
```

Statements end with `;`, `#` starts a comment, objects are declared before
use. `o` and `id` are reserved names. Object and arrow names share one
namespace.

## 7. Storage

The hot operation is a step: read one frame, choose, compose. Frames are
stored CSR (one contiguous slice per object). M1 keeps the category in
memory. M2 adds an immutable rkyv snapshot loaded by mmap, plus an
append-only delta log for verified additions, compacted into a new
snapshot. No database until live multi-writer editing is needed.

## 8. Decisions

| # | decision | why |
|---|---|---|
| D1 | Rust (edition 2024) | memory safety for LF security review; same `egg` as OSIL; C ABI + PyO3 reach C/C++ and Python |
| D2 | pointer = arrow; object + outgoing arrows = decision frame | direct fit for a Choice over a closed option set |
| D3 | standalone project, OSIL bridge later | separate release cycle; share egg-level machinery |
| D4 | MIT + DCO, no CLA | matches OSIL; OSI-approved (no explicit patent grant; revisit before foundation submission) |
| D5 | in-memory CSR now, rkyv/mmap snapshot next | the step is the hot path |

## 9. Milestones

- **M1 (done):** core store, typed paths, egg equality, two-tier walker with
  scripted/uniform choosers, `.onto` parser, `onto` CLI.
- **M2:** delta log + proposal verification pipeline; rkyv/mmap snapshot.
- **M3:** functors between categories; multi-category files.
- **M4:** C ABI (cbindgen) and Python bindings (PyO3); Jev `Chooser` adapter
  (Choice primitive over the frame) in Python.
- **M5:** OSIL bridge: onto categories as OSIL category-level requirements.

## 10. Open questions

- How frames should treat arrows that are *derived* by an equation (should
  a defined composite appear as a choice?).
- What the pairwise MECE verifier asks the System-1 model, exactly.
- The confidence measure: raw top probability (M1) vs. the model's own
  concentration-based confidence.
