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
- **Attested observations are verified.** `attested errors.stopped ==
  true` holds only on an Ed25519-signed observation by a declared
  attester, authorized for that field, bound to this case. A result is
  presented as completion in the world only on such evidence.
- **Open world is the default (after the authority milestone).** Models
  extend the graph by themselves, into a separate *learned layer* that is
  proved against the policy on every load and can never change it
  (§9). `--closed-world` restores the human-only path.
- **The human step is procedural.** Promotion is a command a person is
  meant to run; nothing authenticates who runs it.

## 2. What is trusted, and what is not

| layer | examples | trust | who may change it today | how it is checked |
|---|---|---|---|---|
| **Policy declarations** | `capability … { issuers; revokers }`, `entry … needs / require`, `invariant`, `closed:`, `start:`, arrow `require` | **trusted: the root of trust** | anyone who can edit the `.onto` file | nothing checks *whether a policy is right*; checks only that the graph obeys it |
| Graph structure | objects, arrows, instructions, equations | trusted once committed | file edit, or `onto promote` (one arrow at a time) | loading validates everything below against the policy |
| Case facts | the job's JSON (`consent.given`, `processor.region`) | **asserted by the submitter; not verified** | whoever submits the job | `require` evaluates them; nothing checks they are true |
| Attested observations | signed claims in the case (`observations`) | **verified**: declared attester, strict Ed25519 signature over canonical JSON, field authorization, bound to the case `id` | the attester (holder of the secret key) | `attested` preconditions read only the attested view; rejected observations are reported with the reason |
| Attester declarations | `attester Name { key; observes; }` | **policy: root of trust** | whoever edits the `.onto` file | the policy validator (unique names, valid keys, every attested field observable) |
| Model judgments (System 1) | Jev's Choice, Noul, Score answers | **untrusted, calibrated** | the model | gated: code filters first; confidence thresholds; abstention escalates |
| Proposals (System 2) | an OpenRouter model's suggested arrows | **untrusted** | the model | open world: admitted to the learned layer when no check fails; closed world: provisional; promoted into policy only by a person |
| Learned layer | `<stem>.learned.jsonl` | **untrusted structure, proved against policy** | the open world (any run), or anyone who edits the file | replayed through the structural proofs and the progress rule on every load; arrows that fail are retired |
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
| **Attested observation** | strict Ed25519 verification of `onto-observation-v1` + canonical JSON (attester, case, time, claim); field authorization; case binding | relative to the attester declarations (policy) and the secrecy of the attesters' keys |
| **Attested completion** | a join is labelled `completion attested` only when every branch arrived by an attested arrow; `ask` says "Completed … attested by …" only then | a statement about signed evidence, not about the attester's honesty |

**Not proved, only judged or asserted:**

- that a model chose the right arrow (it is calibrated, gated, and it
  abstains, but it is not verified);
- that case facts are true (§2);
- semantic invariants (`rule "…"`), duplicates and overlap;
- that a description means what its author intended;
- that a `closed:` frame is really complete (an assertion; a proposal
  into it is a *challenge*, and only a person can revise the claim);
- that a successful join reflects **completion in the world** unless it
  is labelled `completion attested`: an unattested join means every
  branch *reached* the join object, nothing more;
- that an attester is honest, that its clock is right, that its key is
  not compromised: observations have no freshness bound or key revocation
  yet, and the time is the attester's own claim.

## 5. Snapshot atomicity

What holds today:

- A run loads the category **once**; every walk, judgment and proposal
  in that run sees one immutable snapshot. A promotion during a run does
  not affect it.
- `onto promote` re-proves structural checks (well-formedness,
  capability authority, invariants) against the **current** file, and
  reloads the edited file before writing, rolling back if it would not
  load.

The three gaps frozen with the milestone are **closed** (after
`8206348`):

| gap | fix |
|---|---|
| records and reviews did not say which graph they judged | every frame record, run report, `run.start` event and review carries the **snapshot**: the SHA-256 of the category source (`shasum -a 256 file.onto` reproduces it); review notes proposals made against another snapshot; the promotion comment names the snapshot the review judged |
| `promote` did not re-run semantic checks | `promote` refuses unless the review's snapshot equals the file's current snapshot: a changed file means re-review (which re-runs rules, duplicates and overlap). No override. |
| promotion was not atomic | an exclusive lock file serializes promotions of a file; the snapshot is checked under the lock; the write is compare-and-swap (re-hash immediately before) and atomic (write a sibling, rename over) |

Residual: the lock serializes `onto promote` runs, not arbitrary editors;
an editor writing between the compare and the rename (a very small
window) is not detected. Snapshots identify content, they do not
authenticate who produced it (signatures are future work, §3).

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

## 7. Decisions reserved for legal review

| question | current behaviour until decided |
|---|---|
| May attested evidence decide a transition without the judge ("decided by evidence"), e.g. the single eligible arrow of a closed frame? | no: the judge decides; an unsure judge escalates to a person |

## 8. For ethicists: where people are in the loop

- A person approves every change to the **policy** (the `.onto` file).
  In the open world (default) models also extend the **learned layer**
  without a person, under the proofs of §9; in the closed world a person
  approves every change to the graph. No model ever promotes into policy.
- A person must look at every `unknown` verdict before it can be
  promoted, and every closure challenge revises a claim only through a
  person.
- The beneficiary sees why: options closed by evidence are named, AI
  suggestions are marked *not active*, and an undecided case is "needs a
  person", not a guess.
- The honest limit: these are **process** guarantees today, not
  authenticated ones (§3, §6).

## 9. Open world: the learned layer

Added after the freeze (D37–D40 in `docs/00-architecture.md`). Stopping
at every missing enumeration kept the index from growing; the default is
now the open world, and the safety argument moves from "a person
approves every arrow" to "every arrow is proved against a policy only a
person writes".

What holds:

- **Policy is untouchable by runs.** Capabilities, entry contracts,
  invariants, attesters, closure claims and roots live only in the
  `.onto` file; runs append only to the learned layer.
- **Every learned arrow passes the same structural proofs** as a promoted
  one (well-formedness, capability authority, `via`/`never`), re-run
  under the graph's write lock against the graph it extends, and again on
  every load. A policy change retires learned arrows that no longer pass.
- **Progress.** A learned arrow may not close a cycle.
- **Budget.** At most `max_expansions` frames per walk (default 3).
- **Labelled.** Records mark learned candidates and `expanded` outcomes;
  `ask` says "through learned structure (not declared policy)".
- **Attestation is unchanged.** A learned arrow has no `attested`
  precondition and no capability effects, so it never produces
  "Completed … attested"; it cannot open a declared entry contract a walk
  does not already satisfy.

What changes, stated plainly:

- **Critic `unknown` admits.** In the open world, a semantic check the
  critic cannot decide does not block learning (in review it blocks
  promotion). Only failures refuse.
- **Closure claims are extended, not revised.** A learned arrow out of a
  `closed:` frame is a closure challenge that the learned layer accepts;
  the declared claim is unchanged, and `onto learned` / `onto review`
  show it.
- **Routing may now use structure no person has seen.** A walk may reach
  an outcome through learned arrows; the answer says so. Where that is
  unacceptable (regulated decisions), run `--closed-world`, or keep
  the relevant region `closed:` and guard its outcomes with entry
  contracts and `attested` preconditions, which learned arrows cannot
  satisfy on their own.
- **The learned layer is a file.** Anyone who can write it can add
  structure; it is still proved against the policy on load. Learned
  layers are git-ignored by default; commit one deliberately.

