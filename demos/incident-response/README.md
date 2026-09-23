# Incident Response

**Mitigation plans race; verification must all pass; completion is claimed
only on signed evidence.**

**Human problem.** During an outage, responders try one mitigation at a
time, or several without coordination, and "it looks fine now" gets
reported as resolved before anyone checked. Customers pay for both.

**Beneficiaries.** The *time-critical* customer (the first mitigation that
is actually applied wins), and the *indirectly protected* customer (the
incident is not declared over until every check is attested).

## How it works

```
Outage  (noul parallel: every plausible plan is tried at once)
  ├─ try_rollback ─▶ Rollback ─ rollback_applied (attested deploy.reverted, DeployBot) ─┐
  ├─ try_failover ─▶ Failover ─ failover_applied (attested replica.promoted, DbOps) ─────┼─▶ Mitigating  join: race
  └─ try_throttle ─▶ Throttle ─ throttle_applied (attested limits.active, EdgeProxy) ─────┘
Mitigating  (split: every check runs; no model decides which apply)
  ├─ ErrorCheck    ─ errors_ok   (attested errors.normal, Monitoring)   ─┐
  ├─ LatencyCheck  ─ latency_ok  (attested latency.normal, Monitoring)  ─┼─▶ Verified  join: all
  └─ CheckoutCheck ─ checkout_ok (attested checkout.passes, Synthetics) ─┘
Verified ─ resolve ─▶ Resolved            invariant via: Outage -> Resolved through Verified
```

Public keys of the five attesters are in `incident.onto`; secret keys are
not in the repository. `jobs` carries observations pre-signed for four
cases.

```sh
onto run demos/incident-response/incident.onto --jobs demos/incident-response/jobs --policy shared --max-branches 6
onto laws     demos/incident-response/incident.onto   # never resolved without verification, for every walk
onto quotient demos/incident-response/incident.onto   # fully asymmetric: every state told apart by structure
```

## Observed (2026-09-23, live)

| case | evidence | outcome |
|---|---|---|
| INC-5001 | rollback + all three checks attested | rollback applied → split into 3 checks → **completion attested**; the final "declare resolved" left to a person (judge unsure) |
| INC-5002 | none | rollback and failover pursued in parallel; both stopped (`unattested`); nothing is claimed |
| INC-5003 | rollback + errors + latency; **no checkout attestation** | checkout check stops → `incomplete_join`: **not verified** |
| INC-5004 | rollback **and** failover attested, all checks attested | the plans **raced**; failover won (rollback "lost the race"); split into 3 checks → **completion attested** |

Two things the live runs taught the runtime:

- A **noul** verification frame let the judge skip the checkout check
  (judged "not applicable"), and the all-join completed without it.
  Verification is now a **split** frame: every eligible arrow runs, no
  model decides, and if the branch budget cannot cover them all the walk
  escalates (`split_over_budget`) instead of dropping one.
- A race winner kept its "waiting at the race" status, so the next join
  saw it as gone and declared itself incomplete. Fixed; a regression test
  reproduces the ordering.

## What it does not do

It does not perform mitigations; it governs which are pursued, which one
counts, and when completion may be claimed. Resolution is declared by a
person. Attestations are only as honest as the attesters (see
`docs/04-trust-model.md`).
