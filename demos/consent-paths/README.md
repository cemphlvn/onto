# Consent Paths

**Prove whether an intended data use has a valid authorization path.**

**Human problem.** Personal data is reused far from where it was collected.
Whether a new use is allowed depends on a chain of legal bases, transforms
and agreements that usually lives in people's heads or a spreadsheet. The
person whose data is reused cannot see that chain, and often no one checks it.

**Pre-offered value.** Bring your data flows; get a map of every authorized
route to each intended use, the routes that are equivalent, and the uses
that have **no** valid path.

**Operators / builders:** privacy engineers, lawyers, data stewards.
**Community seed:** community-maintained consent and policy primitives.

Not legal advice: the category encodes one organization's stated bases.

## What it proves today

Each arrow is a processing step with a legal basis. A use is authorized
only if a path reaches it; equations record routes that end in the same
authorized state.

```sh
C=demos/consent-paths/consent.onto
onto reach $C Collected Published                    # 2 routes, 1 distinct up to equations
onto reach $C Collected Marketing --avoid Consented  # no path: marketing requires consent
onto reach $C Collected Processor                    # 2 routes, one per legal basis
onto run   $C --jobs demos/consent-paths/jobs --policy shared
```

```
2 path(s) Collected -> Published, 1 distinct up to equations:
  publish.aggregate.anonymize.pseudonymize.consent  ≡  publish.aggregate.anonymize.fulfil_pseudonymize.contract
no path Collected -> Marketing avoiding Consented (max 8 arrows)
```

## Observed run (2026-09-23, live, 6 intended uses)

| intended use | outcome |
|---|---|
| personalised offers by email | authorized: `market.consent` |
| academic study on purchase history | authorized: `research_use.pseudonymize.consent` |
| pseudonymized records to analytics vendor | reached `Contract` (open), escalated |
| aggregate statistics in annual report | escalated at `Collected`; a valid path exists (see `reach`), Jev did not find it from names alone |
| churn model on customer histories | **no path**: escalated, proposal `InternalModeling` |
| raw records to an advertising partner | **no path**: escalated; see below |

**The boundary, visible.** For the advertising case the proposer suggested
`market: Collected -> Marketing`, which **bypasses consent**. It stayed
provisional; the committed graph still proves no path to `Marketing`
avoids `Consented`. Detecting that a proposal *would* break such an
invariant, and rejecting it, is M2 work: invariants declared in the
`.onto` file and checked on every proposal.

## With descriptions, parallel arrows and `require` (2026-09-23, live)

- **Evidence gates judgment.** "Send personalised offers" *with*
  `{"consent": {"marketing": true}}` → **✓ `market.consent`** (conf 0.99).
  The same request *without* that evidence → Consented, where `market` is
  closed by `require` → no route. The model was never asked to guess.
- **Parallel arrows:** `share_dpa` (EU, GDPR 28) and `share_scc` (outside
  EU, GDPR 46) share endpoints; `require` on `processor.region` and
  `processor.scc` keeps them exclusive. `onto reach` lists both routes.
- **Stricter, and more correct:** with legal descriptions, Jev no longer
  assumes consent or contract from the intended use alone: 6 of 6 original
  uses escalate at Collected (before: 2 were "authorized" on names only).
  Lesson: the legal basis is a fact of the case; it should be `require`d,
  not inferred.
- The proposer again suggested a basis this organization does not use
  (`legitimate_interest: Collected -> Marketing`). Still provisional;
  since M2 the supervisor rejects it (see below).

## The supervisor (M2, 2026-09-23, live)

The category now declares what no extension may break:

```
invariant via: Collected -> Marketing through Consented;
invariant via: Collected -> Processor through Pseudonymized;
invariant never: Collected -> Advertiser;
invariant via: Collected -> Pseudonymized through Consented | Contract;
invariant via: Collected -> Research through Consented | Contract;
invariant via: Collected -> Aggregated through Consented | Contract;
invariant rule "…only explicit consent and contract… no other legal basis…";
```

`onto review` on one live run's proposals:

| proposal | verdict | why |
|---|---|---|
| `legitimate_interest: Collected -> LegitimateInterest` | **reject** | violates the legal-basis rule (Jev, p 0.96) |
| `research: Collected -> Research` | **reject** | proof: counter-path `research` skips consent and contract |
| `pseudonymize: Collected -> Pseudonymized` | **reject** | reused name, and proofs: it would open Pseudonymized, Research and Aggregated without a legal basis |
| `email_offers: Consented -> Marketing` | **reject** | overlaps `market` (p 0.71): would break Consented's MECE claim |
| `personalize_offers`, `verify_disclosure` | unknown | overlap judgments in the unsure band |

`onto promote` refused the rejected proposal, required
`--override-unknown` for an unknown one, wrote it with a provenance
comment, and refused the same review once it was stale.

## What it does not do yet

- Semantic checks are conservative: no proposal from this run was
  admitted without a person.
- No question-to-target mapping yet: "can you sell my data?" is still
  answered by walking, not by proving non-reachability.
