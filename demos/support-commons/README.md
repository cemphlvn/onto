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

## What it does not do yet

- Proposals are **provisional**: nothing is added to the taxonomy (M2).
- Some proposals point backwards (`Outage -> Ticket`) or duplicate existing
  arrows; M2 verification must reject those.
- Arrows carry names only. Jev sees `follow technical to Technical`, not
  what "technical" covers, which likely explains the CSV and SSO misroutes.
  Arrow descriptions in the `.onto` format are the fix.
