# Support Commons

**Discover support cases your taxonomy cannot handle.**

**Human problem.** Support taxonomies are written once and then quietly
fail. A ticket that fits no category is forced into the nearest one,
misrouted, and the gap never becomes visible to the people who own the
taxonomy. The customer whose request fits nowhere pays for it.

**Pre-offered value.** Bring five real tickets; get a private map of where
your routing taxonomy breaks: which frames escalated, why (`open_frame`,
`none_of_these`, `low_confidence`), and what categories were proposed.

**Operators / builders:** support leads, CX engineers, AI-agent builders.
**Community seed:** anonymized missing intents and routing taxonomies.

## What it proves today

`support.onto` claims (closed frame) that every ticket is billing,
technical, account or product. System 1 (Jev) routes tickets through the
taxonomy; where it cannot, the walk escalates, and those escalations are
the missing intents.

```sh
onto run demos/support-commons/support.onto --jobs demos/support-commons/jobs --policy shared
onto ls  demos/support-commons/support.onto Ticket
```

## Observed run (2026-09-23, live, 9 tickets)

| ticket | outcome |
|---|---|
| charged twice, refund | routed: `issue_refund.refund.billing` → Resolved |
| API 503s since morning | routed to Outage, then escalated: `status_page → Resolved` doesn't fit a live outage |
| export reports as CSV | escalated at Ticket (`none_of_these`); proposal: `how_to: Ticket -> HowTo` |
| reset email never arrives | Account (open) → proposal `recover: Account -> PasswordRecovery` |
| add Okta SSO | misrouted to HowTo, then escalated; proposal: route to FeatureRequest |
| package never arrived | **missing intent**: `delivery: Ticket -> Delivery` |
| delete my account and data | **missing intent**: `request_deletion: Account -> DataDeletion` |
| transfer subscription | **missing intent**: `transfer: Ticket -> SubscriptionTransfer` |
| screen reader doesn't work | **missing intent**: `accessibility: Technical -> Accessibility` |

Wall 10.7 s for 32.5 s of model time (3.04x parallel); 17 waits, all
System-2 edits at the same frame; peak RSS 15.3 MiB.

## With descriptions (2026-09-23, live, same 9 tickets)

Only meaning was added (the arrows are unchanged). Mean confidence of
followed steps 0.81 → 0.94; walks that escalated 8 → 7.

- CSV export: escalated at Ticket → **✓ `answer.howto.technical`**.
- Okta SSO: misrouted to HowTo → **Product** (correct team; Product is open,
  so it still escalates, see below).
- Subscription transfer: unsure at Ticket → **Account**.
- Unchanged, rightly: delivery (no team covers it), data deletion (Account
  is open), screen reader (no accessibility route).
- New gap exposed: "password reset email never arrives" reaches Account,
  where `login` fits exactly, but open frames always escalated.

## Open frames judged, pending proposals shared (2026-09-23, live)

- Password reset: **✓ `reset.login.account`** (open Account frame, `login` fits, conf 0.91).
- Okta SSO: Product → **FeatureRequest**.
- Escalated 5 of 9 (was 7); 4.12x parallel (was 2.65x); 2 waits (was 10).
- Two delivery complaints in one `onto ask` session: the second proposer
  saw the first's pending `delivery → Delivery` and reused it, so the
  cases group ("someone earlier raised a similar case").

## What it does not do yet

- Proposals are **provisional**: nothing is added to the taxonomy (M2).
- Some proposals point backwards (`Outage -> Ticket`) or duplicate existing
  arrows; M2 verification must reject those.
- Grouping relies on the proposer reusing a pending proposal's name; two
  proposals for the same case written independently can still differ.
