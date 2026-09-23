# onto — Dispositional Semantics

Status: **draft 3 — 2026-09-23.** An evaluation of the proposal to treat
object identity as potential behaviour, following Bird's *Monistic
Dispositional Essentialism* (symposium with Armstrong and Ellis; see also
*Nature's Metaphysics*, 2007), and its translation into mechanisms.
Written against the code at `b1a9c60`. Philosophy is used as a design
discipline, not as proof of an architecture. The paper says nothing about
coalgebra or bisimulation: that is our translation.

What the paper supplies:

- **Potency**: an essentially dispositional property (Armstrong's term,
  which Bird adopts). We use it for the metaphysical notion; `Disposition`
  in the runtime stays the name for what happened to a candidate.
- **Laws drop out** of the essential relations among potencies (§2): the
  target for entry contracts (§3, A).
- **Quidditism** (§3): identity as a primitive fact, so roles could be
  swapped. Onto's qualitative labels are quiddities in this sense (§2.2).
- **The regress** is answered by "appropriate asymmetry in the total set
  of relations among potencies" (§3): a graph-theoretic condition we can check.
- **Multi-track dispositions** should be explained as compounds of simpler
  ones; the best case is "a world of on-off properties standing in simple,
  single-track relations", and "it would be interesting to model such a
  world" (§4). Capability tokens set and cleared by single-purpose arrows
  are such a model (§3, A–B, G).

## 1. What already holds: onto is a coalgebra

**Labels.** An arrow's label is $\ell(a) = (\text{instructions},\ \text{require},\ \text{level})$:
its stimulus, its structured precondition over the case, its place on a
scale. The name is *not* part of the label. $L$ is a fixed set: `require`
reads case fields, never objects of $X$, so no dependent construction is
needed. (If a future precondition names objects, it will be.)

**Functor.** Frames are indexed families, which is what the code stores
(parallel arrows with identical labels and targets are distinct `ArrowId`s):

$$ F(X) = \text{Prim} \times \text{Clo} \times \textstyle\coprod_{n} (L \times X)^{n}, \qquad F(f)(p, c, (\ell_i, x_i)_i) = (p, c, (\ell_i, f(x_i))_i) $$

$F$ preserves identities and composition, so

$$ \gamma : X \to F(X), \qquad \gamma(x) = (\text{prim}(x),\ \text{clo}(x),\ (\ell(a), \text{dst}(a))_{a \in \text{out}(x)}) $$

is an $F$-coalgebra. For **behavioural equivalence** we pass to set
semantics, $F_{\mathcal P}(X) = \text{Prim} \times \text{Clo} \times \mathcal P_{fin}(L \times X)$,
via the natural map that forgets order and multiplicity: an exact
duplicate arrow (same label, same target) then has no behavioural effect.
Provenance stays indexed (every `ArrowId` is recorded).

**Judged layer.** Not one kernel. It is indexed by the case, the current
object, **and the walk's history** (the judge sees `hops` and the branch
`focus`), and shaped by the primitive:

$$ J_{\text{choice}}(c, h, x) \in \mathcal D(A_x + \{\bot\}) \qquad J_{\text{noul}}(c, h, x) \in [0,1]^{A_x} \times [0,1]_{\text{fork}} \qquad J_{\text{score}}(c, h, x) \in \mathcal D(\text{Levels}_x) $$

where $A_x$ is the set of eligible arrows (after `require`), $\bot$ is
`none_of_these`, and a Noul answer is a vector of independent Bernoulli
judgments, **not** a distribution (its entries need not sum to one), plus
the fork judgment.

| layer | object | equivalence | decidable? |
|---|---|---|---|
| static | $\gamma$ (frames, labels, targets) | bisimulation (partition refinement) | yes, cheap |
| judged | $J$, history-indexed | testing equivalence on sample cases and histories | only empirically |

## 2. Two corrections

**2.1 Bisimulation cannot group new proposals.** `DeliveryIssue` and
`MissingParcel` arrive as fresh leaf objects with no outgoing arrows. Every
leaf is bisimilar to every other leaf, so behavioural identity would merge
them all, rightly or wrongly. A proposal's identity lives in its *stimulus
side* (which cases activate it: its `about`), not in future behaviour it
does not yet have. Grouping new concepts stays a semantic judgment (the
supervisor's duplicate check). Bisimulation helps once concepts have behaviour.

**2.2 Structure underdetermines identity in our graphs.** In
`support.onto`, `Refund`, `Invoice`, `Outage`, `HowTo`, `Login` and
`FeatureRequest` are each a closed choice frame with one arrow to
`Resolved`. With labels stripped to structure, all six are bisimilar: pure
structure says a refund is a how-to. Bird's answer to the regress needs
the relational graph to be **asymmetric** (no non-trivial automorphisms),
and our graphs are not. What separates the six is the labels, the
qualitative content the judge reads.

Consequence: behavioural identity is bisimulation **modulo a label
equivalence**, and label equivalence is itself semantic. In Bird's terms,
onto's labels are *quiddities*: their identity is not fixed by their role.

**Judged similarity is not an equivalence relation.** Jev may give
A≈B 0.81, B≈C 0.79, A≈C 0.42: transitivity fails. So:

| source of label sameness | use |
|---|---|
| exact label equality | ordinary bisimulation quotient (diagnostics) |
| judged similarity | a behavioural pseudometric or advisory clusters; never a quotient |
| human-approved equivalence classes | safe input to a quotient |

Never quotient the category directly from pairwise model judgments.
Bisimulation then **propagates** approved sameness: once two conditions
are declared equivalent, the fixed point decides which objects become
equivalent, cycles included.

## 3. Build order

| # | proposal | order | notes |
|---|---|---|---|
| A | **Object entry contracts**: an object states what every walk entering it must hold | 1 | the real "laws drop out": an arrow promoted into `Marketing` inherits its entry contract automatically, so a bypass cannot be *walked* without it |
| B | **Arrow effects / capability tokens**: arrows `ensure` or `revoke` tokens; entries `need` them | 1 (with A) | without effects nothing makes an entry requirement true. Tokens are **capabilities, not facts**: an arrow that stands for a real-world fact (consent given) must `require` the evidence in the case before it `ensures` the token; walking an arrow does not make the world so |
| A+B | **Derived laws**: a dataflow analysis over the graph computes, for every object, the tokens a walk *must* and *may* hold on arrival | 1 (with A, B) | "laws drop out" made checkable: entry contracts plus effects imply statements like "every walk into Marketing has passed `consent` since the last `withdraw`"; arrows whose target's needs can never be met are **dead** |
| D | **Closure challenge**: a proposal into a closed frame challenges its completeness claim; a *validated novel* proposal falsifies or revises it | 2 | challenge is immediate; falsification needs validation (a proposal may be nonsense or a duplicate) |
| B′ | **Exact-label behavioural quotient** (partition refinement), diagnostics only | 3 | duplicate states, the asymmetry check (which objects' identity rests on labels alone) |
| C | **Behavioural difference in review**: ΔB = B(G+p) − B(G) as newly reachable pairs, classes merged or split, tokens newly available | 4 | correction: adding an arrow changes its source's behaviour only if it adds a new (label, target class) pair; an exact duplicate changes nothing under set semantics |
| J | **Judged label-equivalence workflow**: model similarity → advisory clusters → human-approved classes → quotient | 5 | per §2 |
| E | **Factored state** for multi-track objects, intensionally: a record of components (basis, form, purpose, recipient), arrows update one component | 6 | statechart-like orthogonal regions without building the Cartesian product; experiment on consent |
| F | **Counterfactual queries** | 7 | queries over γ, tokens and `reach` |

### Status

A, B, A+B and D are **built**. On consent-paths (`onto laws`): one entry
contract on `Pseudonymized` (needs `LegalBasis`) makes "every walk into
Research, Processor, Anonymized has taken consent or contract, and no
withdraw since" derivable though those objects state nothing; all five
`via` invariants hold for walks as well as for the graph; walks started
past the evidence check are blocked (`blocked_by_entry`). Unprompted, the
analysis also shows that walks into `Aggregated`/`Published` need no
legal basis: they may start at `Anonymized`, and anonymized data is no
longer personal data. One semantic fix came out of the work: **starting
at an object is entering it**, otherwise a mid-graph start skips its
contract. Limit: derived laws list every arrow that can grant a token,
not the ones on the paths into that particular object.

## 4. The resulting shape

```
state carries capabilities (tokens)        facts stay in the case
arrow requires facts and capabilities      require · entry needs
arrow manifests a transition               judged: choice · noul · score
arrow produces / revokes capabilities      ensures · revokes
target imposes inherited obligations       entry contracts
M2 checks the behavioural delta            proofs, derived laws, review
```

It connects coalgebraic behaviour, typestate, effect systems, capability
security, supervisory control and decision provenance. The sentence to
keep: *category theory describes how realised paths compose; coalgebra
describes states through their possible future behaviour; M2 governs
changes to that behavioural universe.*

## 5. What this does not imply

Software objects do not have essences; judged probabilities are not laws;
nothing here follows for active inference or cortical models. The
discipline is: *represent states by their structured capacities and
constraints, derive system behaviour from their composition, and say which
parts of identity are structural and which are judged.*
