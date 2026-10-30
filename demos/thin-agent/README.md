# Thin agents on a shared substrate

An architecture rule written as a standard (`standards/thin-agent.onto`)
and checked with functors: agents are configured from declared
blueprints, speak a typed contract, and reach the substrate and the
world only through it.

The standard has two views of the same rule:

| category | arrows mean | checked against |
|---|---|---|
| `ThinAgentArchitecture` | how an answer flows at run time | `onto.onto`, drawn by hand |
| `ThinAgentDependencies` | who can reach whom at build time | `code.onto`, derived from the sources |

## Files

| file | what it is |
|---|---|
| `onto.onto`, `conforms.onto` | onto's architecture as drawn, mapped into the flow view; passes |
| `shortcut.onto` | a tool agent that writes to the world itself; fails authority and invariants |
| `roles.toml` | the human part of the derived check: which role each crate plays |
| `derive.py` | reads `cargo metadata` and the sources, writes `code.onto` (category + functor) |
| `code.onto` | GENERATED: onto's crates, their dependencies and world effects, with evidence |

```
python3 demos/thin-agent/derive.py          # regenerate code.onto
onto functor demos/thin-agent/code.onto     # the verdict
```

`derive.py` maps each arrow onto the shortest route the standard allows
between the two roles. Where the standard allows none, it maps the arrow
to `id`, and onto refuses the functor, naming the arrow.

## The finding (2026-10-30)

The drawn architecture conforms. The derived one does not:

```
error: functor Conforms: requires invariants: ThinAgentDependencies's
`via: Agent -> Substrate through Contract` fails here:
`onto_local_uses_onto_core : OntoLocal -> OntoCore`; … fails here:
`onto_remote_uses_onto_core : OntoRemote -> OntoCore`
```

Both agent crates name onto-core directly: `walk::Answer`,
`walk::Distribution`, `walk::Proposal`, `Primitive`,
`category::NONE_OF_THESE`. These are the contract's own vocabulary, but
they live in the engine's crate. With those two arrows removed, the
derived architecture conforms: those are the only violations.

Two ways to close it, a design decision, not made here:

1. `onto-models` re-exports that vocabulary and the agents depend only
   on `onto-models`: the code moves to the drawing.
2. The vocabulary moves into its own crate with its own role: the
   standard gains an object, and the drawing moves to the code.

## Limits

- Granularity is the crate: a dependency is an arrow whatever it uses.
- World effects are read by pattern (durable writes, processes,
  network), not by data flow; reads are not effects.
- A network call is `Remote`, never `World`: from source alone,
  inference and a side effect over HTTP look the same.
- Rust workspaces only; a Python project (the `fonto` lines) needs its
  own reader of imports and effects.
