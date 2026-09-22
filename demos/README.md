# Onto Commons

**Open decision maps for systems that meet situations their designers did not foresee.**

Every demo shows one promise:

> Many decisions proceed concurrently; possible conflicts become visible
> instead of silently corrupting the shared structure.

Proposal validation and promotion are not built yet (M2). Demos promise
**coordination, detection and inspectability**, not autonomous ontology evolution.

## Demo boundary

Every demo keeps four kinds of structure visibly apart:

| | meaning | where it shows today |
|---|---|---|
| **Committed structure** | objects, arrows, equations in the `.onto` file | `onto check`, `onto ls`, `onto reach` |
| **Provisional proposal** | a System-2 suggestion, never applied | `provisional …` lines, `proposals` in the report |
| **Detected potentiality** | two walks that could meet (node) or proposed the same concept | potentiality list, `potentiality` telemetry events |
| **Validated addition** | a proposal checked and promoted into the graph | *coming in M2* |

Demos run with `--policy shared`, speculation off: substantially faster
(see `docs/00-architecture.md` §5.4), and the potentiality log keeps every
intersection visible. `--policy exclusive` is the conservative **audit mode**.

## Portfolio

| wave | demo | promise | proves today | status |
|---|---|---|---|---|
| 1 | [support-commons](support-commons/) | Discover support cases your taxonomy cannot handle | missing categories (escalations at closed and open frames) | runnable |
| 1 | [incident-graph](incident-graph/) | Let incident agents investigate concurrently without colliding | concurrent intersections on shared components | runnable |
| 1 | [consent-paths](consent-paths/) | Prove whether an intended data use has a valid authorization path | path validity, equations, `--avoid` proofs | runnable |
| 2 | research-coordinator | Run parallel research agents while exposing overlapping hypotheses and evidence | | planned: needs domain schema |
| 2 | repair-commons | Turn disconnected repair knowledge into composable procedures | | planned |
| 2 | learning-paths | Recognize different educational routes to the same capability | | planned |
| 2 | benefits-navigator | Find when public-support systems have no valid path for a person | | planned |
| 3 | supply-exceptions | Coordinate parallel responses to disrupted supply chains | | later: needs validated promotion, permissions, human approval |
| 3 | disaster-network | Prevent independent response agents from competing for the same scarce resources | | later: same |
| 3 | care-coordination | Reveal incompatible referrals and missing care pathways | | later: same; decision support only |

Together, wave 1 exposes the whole architecture: support-commons → missing
categories, incident-graph → concurrent intersections, consent-paths →
path validity and evidence.

## Layout

Each demo is `demos/<name>/` with its category (`<domain>.onto`), a `jobs`
file (one `StartObject: goal` per line) and a README: human problem,
pre-offered value, what it proves today, how to run it, what it does not do.

```sh
cargo build --release
onto=target/release/onto
$onto run demos/support-commons/support.onto --jobs demos/support-commons/jobs \
    --policy shared --telemetry run.jsonl --report run.json
$onto run … --mock      # offline, no API keys

# the beneficiary's side: one case in your own words, answered in plain terms
$onto ask demos/support-commons/support.onto --from Ticket "my parcel never arrived"
$onto ask demos/support-commons/support.onto --from Ticket     # type cases one per line
```

Live runs need `TYPESAFE_API_KEY` (Jev, System 1) and `OPENROUTER_API_KEY` (System 2).
