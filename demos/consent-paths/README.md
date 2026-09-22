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

## What it does not do yet

- No invariant checking of proposals (above). No provenance beyond the
  comments beside each arrow.
- Proposals are provisional (M2).
