# onto

**A category engine that decision models walk.** Define objects and arrows,
declare which paths are equal, and let a fast System-1 model (such as Jev)
route through the graph. Where the graph doesn't enumerate the options well
enough, the walk escalates to a language model that can propose new structure.

> Early stage (M1.5). Design: [`docs/00-architecture.md`](docs/00-architecture.md) · positioning: [`docs/01-positioning.md`](docs/01-positioning.md) · semantics: [`docs/02-dispositional-semantics.md`](docs/02-dispositional-semantics.md) · joins: [`docs/03-joins.md`](docs/03-joins.md).

## Try it

```sh
cargo build
onto=target/debug/onto
$onto check   examples/triage.onto
$onto ls      examples/triage.onto Request
$onto eq      examples/triage.onto build.plan implement.specify   # Equal
$onto walk    examples/triage.onto --from Request --script report,patch,ship
$onto walk    examples/triage.onto --from Request --script ask     # escalates: open frame
```

## The core loop

Run many walks concurrently. System 1 is Jev (`TYPESAFE_API_KEY`); System 2
is any OpenRouter model (`OPENROUTER_API_KEY`).

```sh
$onto run examples/triage.onto --jobs examples/triage.jobs --mock           # offline
$onto run examples/triage.onto --jobs examples/triage.jobs --mock-proposer  # live Jev only
$onto run examples/triage.onto --jobs examples/triage.jobs \
    --policy shared --telemetry run.jsonl --report run.json               # both live
```

`--policy exclusive` (default) makes walks whose decision frames could
intersect wait for each other; `shared` only serializes edits to the same
frame. Every intersection is logged as a potentiality. `--speculate` starts
System 2 alongside System 1. Telemetry is JSON lines (`jq -r .event run.jsonl`).

## Demos

[`demos/`](demos/) holds Onto Commons, runnable demo projects: support
taxonomies (support-commons), concurrent incident agents (incident-graph),
authorization paths (consent-paths).

## Layout

`crates/onto-core` engine (category, path, equality, walk, parse) ·
`crates/onto-runtime` async core loop (engine, frames, providers, telemetry, mem) ·
`crates/onto-cli` the `onto` binary · `examples/` `.onto` files · `demos/` Onto Commons demo projects ·
`docs/` design (start at `00-architecture.md`).

## License

MIT, with DCO sign-off on every commit. See [CONTRIBUTING.md](CONTRIBUTING.md).
