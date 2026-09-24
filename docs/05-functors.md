# onto — Functors

Status: **phases 1 and 2 built — 2026-09-24**. How categories
relate to each other, and how a walk uses those relations. Written before
the code, like `docs/03-joins.md`.

## 1. Principle

A functor `F: A -> B` translates one reference frame into another: every
object of A (in its domain) to an object of B, every arrow to a path of
B, so that composition and declared equations are kept. In a cortical
reading (inspiration, not a claim): a category is a column's model of a
domain, a walk is movement through its reference frame, and a functor is
how one column's position predicts another's.

One foundation, many uses:

```
F0  declared functor: validated, with image and fiber           (onto-core)
 ├─ static, no model        A versioning · B views · C standards · D legal · E composition
 ├─ in a walk, one category H hierarchy (Choice over groups, then the fiber)
 │                          T transport (empty fiber → the missing arrows)
 ├─ in a walk, many         V voting (columns agree) · S column switch (adjunction)
 └─ finding functors        K discovery (quotient + Jev + LLM)
```

Using a functor is code. Choosing is Jev (Choice, Noul, Score). Only
finding an undeclared functor may need an LLM, and code still proves it.

## 2. Syntax

A file may hold several top-level blocks and import others:

```
import "../standards/change-control.onto";

category SecureInfrastructureChange { … }

functor Controls: SecureInfrastructureChange -> ChangeControl {
    objects: ChangeRequested -> Requested, Change -> Assessed, Tests -> Assessed,
             Verified -> Verified, Board -> Verified, Cleared -> Authorized, Deployed -> Implemented;
    capabilities: DeployGrant -> Authorized;
    tested: verify;                 # an arrow maps to a path of B: `g.f`, or `id`
    test_line: id;
    require: contracts, invariants, authority;
}
```

- `objects:` the object map. Objects not listed are **outside the
  domain**: the functor is defined on the full subcategory of mapped
  objects (every arrow between two mapped objects must be mapped).
- `name: path;` maps an arrow to a path of B (`h.g.f`, applied right to
  left as everywhere in onto) or `id`.
- `capabilities:` maps tokens of A to tokens of B.
- `require:` turns reports into load errors (§3).
- `by name;` maps every unlisted object, arrow and capability to the same
  name in the target where it exists (versions). For such functors the
  report also names **relabelled** arrows and frame questions, and
  **changed frames** (options gained or lost).
- A file is loaded as a module: `onto <cmd> FILE` uses the first
  category the file itself declares, `FILE#Name` any other (declared or
  imported). Imports resolve relative to the importing file; cycles fail;
  a file imported twice loads once.

## 3. Semantics and checks

Always required (the functor does not load otherwise):

1. **Well-defined**: every mapped object exists; every arrow between
   mapped objects is mapped; `F(f)` runs from `F(src f)` to `F(dst f)`.
2. **Equations**: for every equation `p = q` of A inside the domain,
   `F(p) = F(q)` in B by equality saturation. `Distinct` fails; `Unknown`
   is reported, never guessed.

Reported, and required when listed in `require:`:

| check | meaning | why it matters |
|---|---|---|
| `authority` | every capability B's image arrows issue has a source: if `F(f)` ensures `T'`, `f` ensures some `T` with `F(T) = T'` | the target's authority is not granted by arrows that have none |
| `contracts` | B's entry contracts are **reflected**: if `F(x)` needs `T'`, `x` needs some `T` with `F(T) = T'` | the source enforces what the target requires at that point |
| `invariants` | B's structural invariants are **reflected on objects**: `via: P -> Q through R` in B holds in A for the preimages of P, Q, R (and `never` likewise). A source arrow whose image passes through R but whose path in A visits no preimage of R does not count: a step the standard requires must be a visible step of the source | the standard's ordering (tested before implemented) holds in the source |
| `cover` | every arrow of B is the image of some arrow | nothing the target describes is missing from the source |

Always reported: **coverage** (B's objects and arrows with no preimage:
gaps, or new behaviour for a version), **evidence backing** (for each
arrow of B, the source arrows mapping onto it and whether each is
attested), and token **preservation** (`f` ensures `T` ⇒ `F(f)` ensures
`F(T)`).

Nothing about `state`/`unseen` is transported yet: a functor does not
change what a model sees.

## 4. Static uses (phase 1)

- **Standards (C):** a shared category (a change-control objective, a
  taxonomy) and a functor from an organization's policy into it. The
  checks above are the compliance report; `cover` gaps are missing
  controls. Standards are ordinary `.onto` files, imported.
- **Views (B):** `onto run --view F` prints each walk's image: a status
  page derived from operations, which cannot contradict them (functors
  keep paths paths).
- **Versions (A):** `functor Upgrade: V1 -> V2`. Coverage of V2 is the
  new behaviour; unmapped V1 structure is what was removed; `onto
  migrate` moves the learned layer and the memory file along the object
  map instead of retiring them.

**As built (phase 1):** `onto functor FILE [NAME]` prints the report;
`onto run --view FILE#F` prints each case's image, as far as it got;
`onto migrate FILE#F --learned … --to-learned … --memory … --to-memory
…` renames learned arrows along the object map and replays them against
the new version, and moves precedents to the image frame and option,
dropping those recorded at a frame whose options changed
(`--keep-stale` keeps them).

## 5. In a walk (phase 2)

**Hierarchy (H).** `frame Ticket: choice grouped by Teams;` where `Teams:
Support -> Coarse`. At `Ticket`, Jev first chooses among the coarse arrows
that are images of `Ticket`'s arrows (few, MECE), then only the fiber
(arrows mapping onto the chosen one) is judged. Both answers are in the
frame record. A confident "none of these" at the coarse level escalates
without the second call.

**Transport (T).** `functor F: A -> B { …; transport; }`. When a walk
escalates at `x` for a missing enumeration, before any LLM: every arrow
`g` of B leaving `F(x)` that no arrow of `x` maps onto is an **empty
fiber**, a direction B knows and A lacks. Each becomes a proposal
`g: x -> y` (y: the unique preimage of `g`'s target, else a new object),
reviewed and admitted like any learned arrow, marked `transported from
F:g`, and the frame is re-judged. B's closed claim at `F(x)` is what
makes this a *completion* of A's enumeration, not a guess. Only when
nothing is transportable, or the re-judged frame still escalates, is the
LLM proposer asked.

**As built (phase 2):**

- *Hierarchy.* The coarse step records `grouped` (functor, groups,
  choice, p, confidence, fiber size, fallback) on the frame record;
  arrows outside the fiber are recorded `other_group`. A coarse answer
  below the confidence gate falls back to the whole frame. **Backoff:**
  if the fiber holds nothing ("none of these"), the frame is judged once
  more without grouping (a wrong group cannot strand a case). Live on 40
  intents: flat 22/22; grouped 21/22 without backoff, 22/22 with it,
  ~20% fewer tokens, twice the calls (`demos/support-commons`).
- *Transport.* Before the LLM proposer, empty fibers become proposals
  (`rationale: transported along F …`), reviewed and admitted like any
  learned arrow; the step's `source` is `transport F`; learned entries
  carry `transported: F:g`, from which a later run restores the images
  of transported arrows and objects. **Settled by structure:** when
  `F(x)` is closed in the target, duplicate and overlap questions
  between a transported option and siblings mapping onto *other* options
  of that frame are passed with the reason, not asked of the critic (the
  closed frame is policy people declared; live, the critic had refused
  both catalogue options against `none_apply`). Transport refusals are
  recorded when the walk falls through to the LLM.

## 6. Phase 3: ensembles (built, 2026-09-24)

Several categories walk the **same case** independently, each a column
with its own judge, its own `state` policy (what it may see), and a
functor into one **shared** category. Code, not a model, compares where
the columns are.

```
ensemble Incident {
    shared: IncidentState;
    column Metrics:    MetricsView    from Signal;   # functor MetricsView: Metrics -> IncidentState
    column Logs:       LogsView       from Entry;
    column Complaints: ComplaintsView from Report;
    consensus: all;                                  # all | quorum N
}
```

- **Position.** A column's position is the image of its walk's current
  object in the shared category (when inside its functor's domain).
- **Agreement is structural.** Two positions are compatible when equal,
  or when one is reachable from the other in the shared category (one
  perspective is only further along). They are a **surprise** when
  neither reaches the other: the perspectives went down different
  branches. The shared category's structure, declared by people, defines
  contradiction; no model judges it.
- **Independence.** Columns never see each other's answers (each judge
  sees its own column's state), so agreement is evidence, not an echo.
- **Outcome per case.** `all`: every column's final position is pairwise
  compatible; the agreed position is the furthest one. `quorum N`: at
  least N columns form a compatible chain; the others are reported as
  dissent. Otherwise: surprise.
- **Surprise is routed by admission.** At an `assured` or `sealed` shared
  frame the case stops for a person ("the perspectives disagree"). At an
  `open_world` one it also becomes a gap signal of kind `contradiction`
  for curation: a sign the model of the world is wrong somewhere. It is
  the loop's third branch.
- **Telemetry and raster.** Each column's positions, the moments of first
  confirmation and first surprise; one raster band per column plus the
  shared band.
- **As built.** `onto ensemble FILE#Name --jobs JOBS` runs one engine per
  column (each with its own judge, learned layer and gap queue:
  `--learned x.jsonl` becomes `x.<Column>.jsonl`), the same jobs in each,
  and prints each case's outcome; `--report` writes it as JSON. A case
  whose columns agree only where they started is **undecided**, not
  agreed; a confirmation needs every column to have concluded something.
  A contradiction's gap signal names the **divergence** (the furthest
  shared object that reaches every position) as its frame, with that
  frame's options, and its key names the conflicting positions (D58,
  D59). Tests: `crates/onto-runtime/tests/ensemble.rs` (agreement,
  blind spot, competing causes, assured → a person, quorum dissent,
  undecided).
- **Demo** (`demos/incident-response/perspectives.onto`, live, Jev): a bad
  deploy → agreed on BadDeploy (metrics and complaints say only
  Degraded, logs go further); a login outage invisible to metrics and
  logs → surprise Healthy ⟂ Degraded at Observed, a curation signal
  (a monitoring blind spot); a campaign surge with pool exhaustion →
  surprise CapacityShortfall ⟂ DatabaseFault at Degraded (competing
  causes); a quiet evening → agreed on Healthy. 12 judge calls, no
  proposer calls, 0.72 s. A first live run left the columns' leaf
  objects open and the open world extended every one of them (14 learned
  arrows, 19 s): a column reports a perspective, so its leaves are now
  `closed` and only its entry frame may learn new kinds of signal.
- **Consensus policies (A2, built).** Only columns that **concluded**
  something count: a column still at its start reaches every position,
  so it would otherwise "agree" without having said anything. `all`
  needs every column concluded and compatible, else `incomplete` (or a
  surprise). `quorum N` needs N concluded compatible columns; the rest
  are **dissent**, and a dissent is still a contradiction: routed like a
  surprise. Under guarded admission an incomplete case also goes to a
  person (D60).
- **Demo** (`demos/hospital-discharge/perspectives.onto`, live, Jev):
  clinical, social and patient columns over an `assured`
  DischargeReadiness; columns `sealed` (their options change only by
  people). `Discharge` (clinical, social; all): fit but no safe home →
  surprise → a person; a new infection with an unsure social column →
  incomplete → a person (the social gap to curation). `WithThePatient`
  (quorum 2): "medically fit" and "wants to go home" outvote "no safe
  home" → agreed on Ready, **with the social dissent sent to a person**;
  a frightened patient with support in place → agreed, dissent → a
  person. 15 judge calls, no proposer calls, 0.73 s. A first run with
  open-world columns learned an arrow at the social frame and took 9.3 s.
- **Later (phase 3b):** column switch (continue in another column where
  this one lacks the enumeration; returning needs an adjoint pair).

## 7. Later

- **Discovery (K)**: candidate object maps from the behavioural quotient
  (same structural profile), Jev Noul per pair, an LLM only for what
  structure cannot match; learned functors in the learned layer.
- **Legal mapping (D)**: the machinery is phase 1; the mapping itself
  needs legal review before any claim is made.
- `state`/`unseen` across functors; natural transformations between two
  functors (two consistent readings of the same case).

## 8. Demos

| phase | demo | shows |
|---|---|---|
| 1 | secure-infrastructure-change → `standards/change-control.onto` | contracts, invariants and authority reflected; evidence backing per control |
| 1 | incident-response → a public status category | `--view`: status derived from operations |
| 1 | benefits-assembly v1 → v2 (a carer circumstance added) | coverage as new behaviour; learned layer and memory migrated |
| 2 | support-commons, taxonomy grown past 40 arrows, `grouped by` a team category | candidates per call, tokens, calibration against a flat Choice |
| 2 | benefits-assembly, carer gap filled by transport from a support-programme category | transport vs the LLM proposer: cost, accuracy, explanation |
