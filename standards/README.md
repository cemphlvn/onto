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

These are sketches in the spirit of common frameworks. They are **not**
the text of any standard, and a functor into one certifies nothing.
Contributions of shared categories are welcome; each needs a README row
and at least one functor that uses it.
