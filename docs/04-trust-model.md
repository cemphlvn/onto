# onto — Trust Model and Proof Scope

Status: **frozen with the authority milestone — 2026-09-23** (commit
`d2773b0`). Written for safety researchers and AI ethicists: what onto
trusts, what it proves, what it only judges, and where its guarantees
end. Every statement below was checked against the code at that commit.
If a statement here and the code disagree, the code is wrong or this
file is stale; both are bugs.

## 1. The short version

- onto **proves** structural properties of its graph and of the
  capabilities walks can hold. It **judges**, with calibrated models,
  which transition fits a case. It **never** lets a model change policy.
- **The policy declarations are the root of trust.** Whoever can edit
  them can authorize anything. Today, editing them is an ordinary file
  edit, governed only by version control and human code review.
- **Case facts are asserted, not verified.** `require consent.given ==
  true` checks what the case *says*. The guarantee is only as good as the
  system that produces the case.
- **The human step is procedural.** Promotion is a command a person is
  meant to run; nothing authenticates who runs it.

## 2. What is trusted, and what is not

| layer | examples | trust | who may change it today | how it is checked |
|---|---|---|---|---|
| **Policy declarations** | `capability … { issuers; revokers }`, `entry … needs / require`, `invariant`, `closed:`, `start:`, arrow `require` | **trusted: the root of trust** | anyone who can edit the `.onto` file | nothing checks *whether a policy is right*; checks only that the graph obeys it |
| Graph structure | objects, arrows, instructions, equations | trusted once committed | file edit, or `onto promote` (one arrow at a time) | loading validates everything below against the policy |
| Case facts | the job's JSON (`consent.given`, `processor.region`) | **asserted by the submitter; not verified** | whoever submits the job | `require` evaluates them; nothing checks they are true |
| Model judgments (System 1) | Jev's Choice, Noul, Score answers | **untrusted, calibrated** | the model | gated: code filters first; confidence thresholds; abstention escalates |
| Proposals (System 2) | an OpenRouter model's suggested arrows | **untrusted** | the model | never applied; reviewed by the supervisor; promoted only by a person |
| Supervisor semantic checks | rule, duplicate, overlap judgments | **untrusted, calibrated** | the critic model | fail ≥ 0.7, pass ≤ 0.3, unknown between; unknown blocks admission |
| Supervisor structural checks | well-formedness, capability authority, `via`/`never` | **proofs** | — | deterministic code, the same code on every route |

## 3. The policy root of trust

An attacker who can edit **both** an arrow and a capability declaration
can write

```
capability LegalBasis { issuers: consent, contract, fake_basis; }
fake_basis: Collected -> Pseudonymized ensures LegalBasis;
```

and the graph is internally valid: every check passes, and the forged
capability flows to every object whose entry contract needs it.
Capability authority protects **against the graph**, not **against the
policy**. So:

| operation | governed by |
|---|---|
| ordinary graph extension (a new arrow, a new object) | the capability declarations, entry contracts and invariants, checked identically on load, in review and at promotion |
| **policy modification** (capabilities, entry contracts, invariants, closure, roots, arrow preconditions) | **a privileged governance operation** |

What onto enforces today: the extension pipeline **cannot** modify policy.
A proposal can only add one arrow (with effects under existing authority)
and possibly a new, open object; `onto promote` writes exactly that. What
onto does **not** enforce today: anything about direct edits to policy.

Policy changes should, and will, require:

1. a separate review class, distinct from arrow review;
2. a **named human authority** (not "a person ran a command");
3. the **snapshot hash** of the graph before and after;
4. a **signed** approval;
5. a full **behavioural-difference report** (reachability, derived laws,
   capabilities gained or lost, equivalence classes merged or split);
6. **no model-only approval**, ever: a model may draft or critique a
   policy change, never approve it.

None of these exist yet. Until they do, protect the `.onto` files the way
you protect access-control configuration: code owners, required reviews,
signed commits.

## 4. What is proved, and within what scope

| claim | how | scope and assumptions |
|---|---|---|
| **Capability authority**: only declared issuers grant, only declared revokers revoke | `validate_capabilities`, one validator for loading, review and promotion | relative to the declarations (§3) |
| **Structural invariants**: `via` (every path passes a gateway), `never` (no path) | shortest counter-path search; the graph does not load if violated; proposals rejected with a counter-path | the graph as written; reachability only, no temporal logic |
| **Derived laws**: tokens every walk holds on arrival; walk-level invariant enforcement; dead arrows | exact exploration of (object, tokens) states; certificates, witnesses, counterexamples | case preconditions are **assumed satisfiable** (over-approximates cases, exact for tokens); two scopes: declared roots (application) or all startable objects (category); cost is exponential in the number of tokens |
| **Starting is entering** | the engine refuses a start where the entry contract fails | runtime engine; the synchronous test walker does not enforce it |
| **Path equality** | equality saturation (egg) | Equal / Distinct / **Unknown** when limits are hit: never a guess |

**Not proved, only judged or asserted:**

- that a model chose the right arrow (it is calibrated, gated, and it
  abstains, but it is not verified);
- that case facts are true (§2);
- semantic invariants (`rule "…"`), duplicates and overlap;
- that a description means what its author intended;
- that a `closed:` frame is really complete (an assertion; a proposal
  into it is a *challenge*, and only a person can revise the claim);
- anything about joins (designed, not built: `docs/03-joins.md`).

## 5. Snapshot atomicity

What holds today:

- A run loads the category **once**; every walk, judgment and proposal
  in that run sees one immutable snapshot. A promotion during a run does
  not affect it.
- `onto promote` re-proves structural checks (well-formedness,
  capability authority, invariants) against the **current** file, and
  reloads the edited file before writing, rolling back if it would not
  load.

Known gaps, frozen as limitations of this milestone:

| gap | consequence | planned fix |
|---|---|---|
| reviews and disposition records do not record which snapshot they were made against | an audit cannot prove which graph a verdict judged | a content hash of the category in every record, review and promotion comment |
| `promote` re-runs structural checks but **not semantic ones** (rules, duplicates, overlap) | a verdict can go stale: e.g. an equivalent arrow promoted since the review | require the review's snapshot hash to match, or re-review |
| `promote`'s read–check–write is not atomic | two concurrent promotions can lose one update | compare-and-swap on the file hash (re-read and compare immediately before writing), or a lock |

## 6. Threats considered

| threat | outcome today |
|---|---|
| a model proposes a consent bypass (`Collected -> Marketing`) | rejected with a counter-path (structural invariant); would be dead anyway (entry contract) |
| a proposal counterfeits evidence (`ensures LegalBasis`) | rejected on load, in review and at promotion (capability authority) |
| a proposal reuses an issuer's name | fails well-formedness (names are unique) |
| a walk starts past an evidence check | refused, or blocked at the next contract |
| **prompt injection in the case text** | can steer which *eligible* arrow a judge picks, and what a proposer suggests; **cannot** open a gated arrow (code decides eligibility first) and cannot promote anything |
| a submitter lies in case facts | **not prevented**: facts are asserted (§2) |
| someone edits policy | **not prevented by onto** (§3) |
| an AI agent with shell access runs `onto promote` | **not prevented**: promotion is procedural (§1) |

## 7. For ethicists: where people are in the loop

- A person approves every change to the graph. Models draft (proposer),
  critique (supervisor), and route (judge); none of them promotes.
- A person must look at every `unknown` verdict before it can be
  promoted, and every closure challenge revises a claim only through a
  person.
- The beneficiary sees why: options closed by evidence are named, AI
  suggestions are marked *not active*, and an undecided case is "needs a
  person", not a guess.
- The honest limit: these are **process** guarantees today, not
  authenticated ones (§3, §6).
