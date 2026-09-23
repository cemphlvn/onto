# onto — The Four Spaces

Status: **reference — 2026-09-24**. Which space each part of onto works
in, which spaces it builds, which it only observes, and which it leaves
to models. Read this before adding a layer: it says where the layer
lives and what it may claim.

## 1. The four spaces

| # | space | what it holds | its nature | how onto treats it | a neuroscience parallel |
|---|---|---|---|---|---|
| 1 | **Structure** (ontological) | which distinctions exist: objects, arrows, MECE enumerations, equations, and the policy around them | discrete, **declared** (plus the learned layer) | **built**; everything about it is checked or proved | the connectome: which connections exist |
| 2 | **State** (functional, dynamical) | every state a walk can be in (object × tokens × history × forks × attested view) and the transitions between them | discrete, large, **derived** from 1 | **built and explored**: the engine walks it; `onto laws` explores it with certificates | neural dynamics: which states are reachable |
| 3 | **Trace** (observed) | what actually happened: frame records, telemetry, reports | empirical trajectories through 2, projected onto 1 | **recorded**; views: `onto why`, `run --view`, the raster (next) | recordings: spike rasters |
| 4 | **Concept** (representational geometry) | how close meanings are: refund vs double charge, maternity vs none | continuous, high-dimensional, **geometric** | **consulted, not built**: models hold it; it enters onto only as typed judgments | representational geometry: neural manifolds, RSA |

```
        4  concept space   ← inside Jev and LLMs; reached only through judgments
            ↑  descriptions, `state`, typed answers (the bridge)
onto ─┬─   1  structure     ← the category, policy, functors, the learned layer
      ├─   2  state         ← the engine walks it; `onto laws` proves over it
      └─   3  trace         ← records and telemetry; views of what happened
```

## 2. Why the boundary sits where it does

Everything onto **proves** lives in 1 and 2: capability authority, entry
contracts, `via`/`never`, `unseen`, sealed regions, functor reflection,
laws with certificates. Everything onto **judges** comes from 4, through
one door: the judged layer J(c, h, x) of
`docs/02-dispositional-semantics.md`, where a model answers a typed
question about a case at a frame. The trust model
(`docs/04-trust-model.md`) rests on this split: a model can move a walk
among eligible arrows; it cannot make an arrow eligible.

Functors are maps in 1 (category to category); they induce maps in 2
(walk states to walk states), which is what the planned prediction across
columns uses. Transport moves structure along them; it never moves
concept-space claims.

## 3. Where each part of onto lives

| part | space |
|---|---|
| `.onto` categories, policy, `admission`, `state`, functors | 1 |
| learned layer (`*.learned.jsonl`) | 1 (hypotheses, with provenance) |
| engine, frame claims, forks, joins, laws | 2 |
| frame records, telemetry, run reports, memory file | 3 |
| Jev's answers (Choice, Noul, Score), the critic's checks | 4 → 1/2, as recorded judgments |
| precedents shown to a judge (`memory: similar N`) | 3, projected through 1's `state` policy, compared lexically (not in 4) |

## 4. The geometry layer: deliberately not built

A layer placing objects and arrows in a concept space (points and
regions, as in Gärdenfors' *Conceptual Spaces*) would have natural uses:
meaning-based similarity for precedents (today lexical), overlap as
region intersection (today one critic question per pair), candidate
functor maps (phase 4 of `docs/05-functors.md`), layouts of large
taxonomies.

It stays **off** until a measured need appears. Candidates for that
evidence, already visible:

- precedents chosen by word overlap (benefits-assembly) may miss the
  closest case in meaning;
- overlap checks blocked admission twice live (catalogue options against
  `none_apply`; settled now by the catalogue's closed claim).

If built, it must stay **advisory**: it may order, suggest or flag, never
admit, prove or authorize; policy and proofs remain in 1 and 2. The
preferred construction keeps onto's discipline: geometry derived from
recorded pairwise judgments (Noul "similar?", Score "how similar?", as in
representational similarity analysis) rather than from an unrecorded
embedding.

## 5. How to use this document

Before adding a feature, name its space. If it proves, it belongs in 1
or 2 and must be deterministic. If it observes, it belongs in 3 and must
not change behaviour. If it needs meaning, it reaches 4 through a typed,
recorded judgment, and what it produces is judged, never proved.
