# Consent Enforcement

**A requested use of personal data proceeds only when its purpose, the
evidence for it, and an authorized capability meet.**

**Human problem.** Consent is checked when data is collected, then
forgotten. Months later someone runs a campaign or a study, and nothing
checks, *at that moment*, whether this person allowed this use. The
person affected is never in the room.

**Beneficiaries.** The *rights-holder* (an appropriate-looking use is
blocked unless authorized) and the *indirectly protected person* (the
data subject never uses this system; they are protected when a use is
attempted, not only when consent was collected).

## How it works

```
UseRequested (choice: judged once — marketing or research?)
  marketing ─▶ MarketingUse (split: two lines at once, no judgment)
                 ├─ prepare_campaign ─▶ Draft ─ drafted ─────────────────────────────┐
                 └─ check_marketing ─▶ MarketingCheck ─ marketing_consented ─────────┤
                      attested consent.marketing == true (ConsentLedger)             ▼
                      ensures MarketingGrant                            MarketingCleared  join: gate
                                                                        authority check_marketing
                                                                        export MarketingGrant
                                                                              │ send
                                                                              ▼
                                                   Sent   entry: needs MarketingGrant
  research ─▶ the same shape: Dataset + ResearchCheck (consent.research by ConsentLedger,
              or ethics.approved by EthicsBoard, either ensures ResearchGrant) ─▶ Study
```

- Capabilities have declared issuers (`marketing_consented`;
  `research_consented`, `ethics_approved`): nothing else can mint them.
- The gate exports **only** the capability for this purpose.
- `Sent` and `Study` state entry contracts: no route, present or future,
  reaches them without the capability. `onto laws` proves it with a
  certificate.
- After the purpose is judged, preparation and the final use are
  mechanical (`split`): onto governs the decision, not the action.

```sh
onto run demos/consent-enforcement/consent.onto --jobs demos/consent-enforcement/jobs --policy shared --max-branches 6
onto laws demos/consent-enforcement/consent.onto --proofs
onto ask  demos/consent-enforcement/consent.onto --from UseRequested '<a line from jobs>' --why
```

## Observed (2026-09-23, live)

| case | evidence | outcome |
|---|---|---|
| C-1 | ConsentLedger attests `consent.marketing = true` | **Sent**: "authorized by attested evidence (ConsentLedger: consent.marketing @ …)" |
| C-2 | ConsentLedger attests `consent.marketing = false` (withdrawn) | **blocked by the gate**: "verified observations do not establish it" |
| C-3 | none | **blocked**: "the case carries no observations" |
| C-4 | EthicsBoard attests `ethics.approved = true` (a study) | **Study**, authorized by the ethics board |
| C-5 | forged: claims ConsentLedger, signed with another key | **blocked**: "rejected ConsentLedger: signature does not verify" |
| C-6 | replay: C-1's genuine observation copied into another case | **blocked**: "bound to case `C-1`, not this case" |

What the first live runs taught: when content steps were judged ("the
campaign is drafted"), the judge would not assert work that had not
happened yet, and after authorization it would not decide to *send*. The
purpose is judged once; after that the steps are mechanical, and the
capability, not a model, decides whether the use may happen.

## What it does not do

It does not send anything or run studies; it decides whether the use is
authorized. It is only as good as the consent ledger's attestations
(`docs/04-trust-model.md`). Not legal advice: the category encodes one
organization's policy.
