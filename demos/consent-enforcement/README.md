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

## What models see (state policy)

Every case carries the person's whole record (name, email, date of
birth, health conditions, purchases), as a real system would. **No model
is shown any of it**, and that is proved, not hoped:

```
state { case: request.purpose, request.description, request.channel; }
state MarketingCheck { case: request.purpose; observed; }
state ResearchCheck  { case: request.purpose, request.study; observed; }
invariant unseen: case.person;
```

- The judge that classifies the use reads only the request.
- At the checks it also reads `observed`: only verified, signed
  observations (the raw, unverified `observations` are never shown).
- `invariant unseen: case.person` is proved when the file loads and for
  every proposal and learned arrow (a new object inherits the default
  state). A frame that would show any part of `person` fails to load,
  naming the frame. `onto laws` prints what each frame sees.
- Every frame record keeps the exact state sent (`seen`); `onto why`
  prints `model saw: …`.

```
what models see   per the `state` policy · case and observed fields proved · goal is free text
  (`proposer`: no judge; a proposer is asked only if the walk escalates there)
  UseRequested      judge     case: request.purpose, request.description, request.channel
  MarketingUse      proposer  case: request.purpose, request.description, request.channel
  ResearchUse       proposer  case: request.purpose, request.description, request.channel
  Draft             proposer  case: request.purpose, request.description, request.channel
  MarketingCheck    judge     case: request.purpose · observed (attested only)
  MarketingCleared  proposer  case: request.purpose, request.description, request.channel
  Dataset           proposer  case: request.purpose, request.description, request.channel
  ResearchCheck     judge     case: request.purpose, request.study · observed (attested only)
  ResearchCleared   proposer  case: request.purpose, request.description, request.channel
  never asked (terminals): Sent, Study
  unseen: case.person   ✓ proved: no frame shows these fields, nor any field inside or around them
```

Live (2026-09-24, open world): the six outcomes below were unchanged; 15
of 37 frame visits called a model, and none of the 15 recorded states
contains any value from the person's record. In C-2 (consent withdrawn),
the open world learned `request_marketing_consent: MarketingCheck ->
MarketingConsentRequest`; the authority branch went there instead of to
the gate, so the campaign stayed blocked: learned structure cannot mint
`MarketingGrant`.

## Where the open world may grow (sealed regions)

```
world: closed;
learnable: UseRequested;
```

Only new *kinds of use* may be learned, at `UseRequested`. Every other
object is sealed: no learned arrow may leave it or enter it, so the
checks, gates and outcomes (`Sent`, `Study`) are reached only by declared
arrows. What the open world learns is an island that needs a person.

Live (2026-09-24): C-1…C-6 unchanged, and C-2 no longer learns at the
sealed `MarketingCheck`. C-7 ("sell the person's contact details to a
data broker") learned `broker_sale: UseRequested -> BrokerSaleUse` and two
review steps after it, then stopped: **needs a person**. No route from
the island reaches `Sent` or `Study`, and the proposer, too, saw only
the request.

Limits: the proof is about **which fields** reach a model, not what free
text says. `request.description` is shown; if a requester writes the
person's diagnosis into it, the model sees it. Keep shown fields
structured where possible.

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
| C-7 | a use the policy does not enumerate (sale to a data broker) | **learned island, needs a person**: new objects after `UseRequested`; sealed outcomes unreachable |

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
