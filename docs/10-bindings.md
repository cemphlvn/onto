# onto — The Engine and Its Python Bindings

Status: **design, agreed direction — 2026-09-24** (M4, plan step D). Not
built yet. Code follows this document.

## 1. What this layer is

onto is an **embeddable engine**: a contract (the `.onto` policy and its
proofs), a runtime (walks, models, admission), and a graph (categories,
functors, the learned library; later possibly a database). The Python
package is its **native bindings**: a thin extension module that exposes
the engine's primitives as Python objects.

Principle: **mechanism, not policy.** The bindings are a capability, not
a framework. They prescribe no way of building an application: no app
object, no decorators, no loop, no place where a person must be asked.
Whoever builds on top decides all of that.

```
   apps · agent frameworks · ComposableAI · DSPy · notebooks · services
   ──────────────────── anything, built by others ────────────────────
                              ▲        ▲
        basic  ───────────────┘        │   convenience functions, written in
                                       │   Python over the primitives below
        advanced (primitives) ─────────┘   every capability of the engine
   ─────────────────────── the narrow waist ───────────────────────────
        onto._onto  (native module: PyO3, one-to-one with the Rust API)
   ─────────────────────── FFI boundary ───────────────────────────────
        onto-runtime · onto-core   (Rust: the single source of truth)
```

The shape is an hourglass: few concepts at the waist, many uses above,
the whole engine below. The model is the Rust-core, Python-surface
projects that do this well: pydantic-core under pydantic (an
unopinionated engine that frameworks build on), Polars, and embedded
engines such as DuckDB, Kùzu, LanceDB, delta-rs and DataFusion (a
library, not a server).

### Precedents (read on GitHub and in the PyO3 / maturin docs, 2026-09-24)

| project | what it does | what onto takes |
|---|---|---|
| pydantic-core | `python-source = "python"`, `module-name = "pydantic_core._pydantic_core"`, a `_pydantic_core.pyi` stub, `py.typed`; `core_schema.py` defines the engine's input as plain typed data (TypedDicts) that pydantic and others build | the layout (`onto._onto`); **declarations as data**: a category can be built from dicts, not only from `.onto` text |
| lancedb | the async Rust API is authoritative; the sync API runs coroutines on a background event-loop thread, "without messing with users' event loops"; blocking callbacks (embedding models) get a dedicated executor because they starved unrelated work (issue 3310); pluggable embeddings, rerankers, integrations | async first, sync over a background loop; Python model callbacks on a dedicated executor; extension points as protocols, integrations outside |
| rustworkx / rustworkx-core | a pure-Rust crate with a stable API "for any downstream crate", apart from the Python package | `onto-core` and `onto-runtime` stay a stable Rust API: a connection point for Rust callers too |
| datafusion-python (`why-ffi.md`), pyo3-polars | Rust has no stable ABI: a Rust extension in another wheel cannot link against the engine; DataFusion crosses an FFI crate and PyCapsules, Polars a plugin ABI | third-party Rust extensions (a Rust judge in someone else's wheel) need a C ABI or PyCapsule boundary: the C ABI's real purpose; bulk data later through the Arrow PyCapsule interface |
| delta-rs, datafusion-python, lancedb | the binding crate in its own directory beside the core crates | `crates/onto-py`, converting only |

## 2. Rules

1. **Rust stays the single source of truth.** The binding crate
   (`crates/onto-py`) converts types and nothing else; no logic lives
   only in Python or only in the binding.
2. **Primitives, not workflows.** Every engine capability is reachable:
   categories, proofs, walks, records, the library, review, replay,
   functors, ensembles, discovery, raster events.
3. **Two levels of use, one set of capabilities.**
   - *Advanced*: the primitives, every parameter.
   - *Basic*: a handful of functions with defaults (load a file, run
     cases with Jev from the environment, read the results), written in
     Python on top of the advanced level.

   Basic has no capability of its own: anything basic does, advanced can
   do and change.
4. **Extension points are protocols.** Any Python object with the right
   method can be a judge, a proposer or a critic, synchronous or
   `async`. The engine calls it with plain data and awaits its answer.
   This is how other libraries connect, without onto knowing them. The
   built-in providers (Jev, OpenRouter, mocks) are Python objects too,
   so they can be wrapped, composed or replaced.
5. **Declarations are data too.** A category, functor or ensemble can
   come from `.onto` text or from plain data (the Rust `Category`
   builder already exists), so libraries above can generate policy
   programmatically; it passes the same proofs either way.
6. **State is values.** The learned library, precedents, gap signals,
   records and reports cross the boundary as plain data (dicts and lists
   matching the JSON the engine already writes, typed in `.pyi` stubs).
   Files next to the `.onto` become one convenience among many; a
   database is another.
7. **Speed is kept.** While walks run the GIL is released (`py.detach`),
   so the tokio runtime keeps its parallelism; it is re-acquired only to
   call a Python protocol object. Async is authoritative (a tokio future
   as a Python awaitable, `pyo3-async-runtimes`); the sync API runs on a
   background loop and never touches the caller's event loop. Python
   model callbacks that block run on a dedicated executor. Events and
   records stream as iterators; bulk records as Arrow later.
8. **Policy stays policy.** The bindings expose promotion, admission and
   the proofs as they are. Nothing in Python can change a policy file
   except what `onto promote` can (`docs/04-trust-model.md`).

## 3. The surface (draft)

What crosses the boundary, per concept. *Exists*: the Rust item is
there. *Engine work*: it must be built first (§4).

| Python (advanced) | Rust | status |
|---|---|---|
| `Module.load(path)` / `Module.parse(text, importer=…)`; `.categories`, `.functors`, `.ensembles` | `parse::parse_module_at`, `Module` | exists (file loading with imports lives in the CLI: move) |
| `Category.build(data)`: a category from plain data | `category::Builder` | exists (no data schema yet: define it, TypedDicts in the stubs) |
| `Category`: `objects`, `arrows`, `out(o)`, `reach(a, b, avoid)`, `paths`, `compose`, `equal(p, q)`, `snapshot` | `Category`, `Path`, `Equality` | exists |
| `Category.laws(roots=…)`, `.quotient(mode)`, `.invariants_enforced()` | `laws::derive`, `quotient` | exists |
| `Category.with_learned(entries) -> (Category, refused)` | `Category::with_learned` | exists |
| `Category.review_structure(proposal)` | `supervise::structural` | exists |
| `Functor`: `build(decl, a, b)`, `.report`, `.image(path)`, `.fiber`, `.empty_fibers` | `functor::Functor` | exists |
| `discover.admissible`, `.search`, `.arrow_images`, `.declaration`, `.render` | `onto_core::discover` | exists |
| `Ensemble`, `ensemble.run(…)` | `ensemble::run` | exists (generic over models: erase) |
| `Runtime(category, judge=, proposer=, critic=, config=)` | `Engine` | exists (generic: erase) |
| `Runtime.run(cases)` → report · `await Runtime.arun(cases)` | `Engine::run` | exists |
| `Runtime.use_library(entries)`, `.remember(precedents)`, `.use_functors(…)` | `use_library`, `remember`, `use_lenses` | exists |
| `Runtime.events()` (iterator / async iterator of every telemetry event) | positions observer only | engine work |
| `Runtime.records()` streamed as they are made | written at run end | engine work |
| `Runtime.resume(record, answer)`: continue a stopped walk | — | engine work |
| `Runtime.step(case, at)`: one frame, no walk | — | engine work |
| `Config` (every field, with defaults) | `Config` | exists |
| protocols `Judge.judge(frame) -> answer`, `Proposer.propose(gap) -> proposals`, `Critic.nouls(state, questions) -> probabilities` | `model::{Judge, Proposer, Critic}` | exists (not dyn-compatible: erase) |
| providers `Jev(…)`, `OpenRouter(…)`, `MockJudge`, `MockProposer` | `providers`, `model` | exists |
| `review(proposal, …)`, `curation.group(signals)`, `curation.request(…)` | `supervisor`, `curation` | exists |
| `replay(record, change)` | CLI `replay` | move into the library |
| `trace.project(events) -> raster`, `.insights` | `trace::project` | exists |
| `attest.keygen()`, `.sign(…)`, `.verify(…)` | `attest` | exists (sign lives in the CLI: move) |

The basic level, for orientation only (names not fixed; nothing here is
a required way of working):

```python
import onto
cat = onto.load("support.onto")                 # module + learned layer from files
report = onto.run(cat, ["my parcel never arrived"])   # Jev / OpenRouter from the environment
for walk in report["walks"]:
    print(walk["path"])
```

## 4. Engine work before the bindings

1. **Type-erased models.** `Judge`, `Proposer` and `Critic` return `impl
   Future`, and `Engine<J, P>` is generic. Add boxed-future adapters
   (`DynJudge`, `DynProposer`, `DynCritic`) so one concrete engine type
   can hold a Rust provider or a Python object.
2. **The world as values.** Move loading and saving of modules with
   imports, the learned layer, lenses, the library, precedents and gaps
   out of `onto-cli` into `onto-runtime`, as functions over values with
   file helpers beside them. The CLI then uses them like any caller.
3. **Events as a stream.** Generalize the ensemble's position observer
   into a subscription to every event the engine emits (the telemetry
   events of `docs/00` §5.3), in process.
4. **Streaming records.** Emit each frame record when it is made (on the
   same stream), not at run end: durable, and a live raster possible.
5. **Resume and step.** Continue a stopped walk from its record with an
   answer given from outside (a person, another system), recorded in the
   frame record with where it came from; judge a single frame without a
   walk.
6. **Replay and signing as library functions** (today in the CLI).
7. **A data form of declarations**: the schema the builder accepts,
   mirrored as TypedDicts (pydantic-core's `core_schema` pattern).

Each item is useful to Rust callers and to the CLI on its own; the
bindings then only convert.

## 5. Build and packaging

- `crates/onto-py`, maturin, mixed layout: `python-source = "python"`,
  `python/onto/` (the basic level, `__init__.py`, `_onto.pyi`,
  `py.typed`) and the native module `onto._onto` (`module-name =
  "onto._onto"`).
- `abi3` wheels (one per platform for every supported Python).
- Free-threaded CPython declared only once the engine's callbacks are
  audited for it.
- A C ABI (or PyCapsule) boundary when third-party Rust extensions need
  to plug into the engine from their own wheels (datafusion's FFI
  lesson); not before.

## 6. Not in this layer (later, separate)

An application library built on the bindings, with opinions: how parts
connect, how far each part may expand itself or what it may admit
automatically, which parts trigger a person, loop architectures and
decision processes, and a developer experience where writing an onto
application by hand is easier than writing prompts. It is a separate
package with its own design, started only after the bindings exist and
people have built on them.

## 7. Test (plan step D)

A developer or an agent, given only the documentation, builds the
consent-enforcement application in Python twice: once with the basic
level, once customizing it through the advanced level (a Python judge
that wraps Jev; the learned library kept in their own store). Measured:
time to a first correct decision, and whether they needed to read Rust.
