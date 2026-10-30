# Shared categories

Categories meant to be imported and mapped into, not walked: an
organization writes its own policy, then a **functor** from its policy
into a shared category (`docs/05-functors.md`). Loading the functor
proves what it `require`s (authority, contracts, invariants reflected);
`onto functor FILE` reports the rest, including what the standard
describes and the policy lacks.

| file | category | used by |
|---|---|---|
| `change-control.onto` | `ChangeControl`: classify, verify, authorize, implement, post-implementation review | `demos/secure-infrastructure-change/controls.onto` |
| `thin-agent.onto` | `ThinAgentArchitecture` (run-time flow) and `ThinAgentDependencies` (build-time reach): agents reach the substrate and the world only through the contract | `demos/thin-agent/conforms.onto` (onto as drawn), `demos/thin-agent/code.onto` (onto as derived from its sources), `demos/thin-agent/shortcut.onto` (a failing counterexample) |

These are sketches in the spirit of common frameworks. They are **not**
the text of any standard, and a functor into one certifies nothing.
Contributions of shared categories are welcome; each needs a README row
and at least one functor that uses it.
