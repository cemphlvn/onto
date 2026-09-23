# Incident Graph

**Let incident agents investigate concurrently without colliding.**

**Human problem.** During an outage, several people and agents chase
different symptoms that lead into the same components. They duplicate work,
contradict each other, or change the same system at once, and nobody sees
the overlap until afterwards. Customers and employees wait longer.

**Pre-offered value.** Bring a past incident's alerts; get a replay showing
where your responders' investigations intersected, who would have waited on
whom, and which components your runbooks do not cover.

**Operators / builders:** SREs, security teams, platform engineers.
**Community seed:** an open library of incident states and response paths.

## What it proves today

Eight agents start at different symptoms (`Alert`, `Latency`,
`ErrorSpike`, `SecurityEvent`, `Network`). Their frames share
`Database`, `Network` and `SecurityEvent`. Under `shared`, reads run
together and are logged as coexisting; two edits of the same frame wait.
`Network` is open on purpose: DDoS, BGP and DNS faults are not enumerated.

```sh
onto run demos/incident-graph/incident.onto --jobs demos/incident-graph/jobs --policy shared
onto run demos/incident-graph/incident.onto --jobs demos/incident-graph/jobs --policy exclusive   # audit mode
```

## Observed run (2026-09-23, live, 8 agents)

- 3.73x parallelism (8.7 s wall, 32.3 s model time); 57 potentialities, 3 waits:
  - two agents proposing new structure at `Network` (packet loss; login flood): 1.1 s wait
  - two agents proposing at `Database` (replication lag; pool exhaustion): 4.6 s wait
  - a credential-leak agent reading `SecurityEvent` while another extended it: 3.6 s wait
- Routed confidently: deploy spike → `rollback.bad_deploy.errors`;
  leaked key → `leaked.security`.
- Gaps found: application-level latency, queueing, replication lag,
  connection-pool capacity, outbound-traffic anomalies, rate limiting.

## With descriptions, a noul frame and a score frame (2026-09-23, live)

`Alert` is now a **noul** frame and `Report` a **score** entry point.

- **Fork:** "checkout is slow for everyone, and separately an API key was
  posted" → fork p=0.92: `security` (0.90) and `latency` (0.85); walk 12
  branched off and investigated latency in parallel.
- **Score:** "whole checkout page is down" → `declare` (level 2, conf 1.00)
  → Major → Alert; "button label misaligned" → `watch` (level 0, conf 0.98).
- Deploy spike now reaches **✓ `verify.rollback.bad_deploy.errors`**
  (before: stopped unsure at Rollback); outbound traffic now reaches Network
  (before: unsure at SecurityEvent).
- 12 walks incl. 1 branch, 3.67x parallel, 25 judge calls carrying 40 questions.

## Joins (2026-09-23, live)

`join Mitigated: all;` — an incident that forked into independent
aspects reaches `Mitigated` only when every aspect's walk does.

**Attested (2026-09-23).** Every arrow into `Mitigated` needs a signed
observation from a declared attester (`DeployBot`: errors.stopped,
service.recovered; `SecOps`: key.old_rejected). The public keys are in
`incident.onto`; the secret keys are not in the repository; the last line
of `jobs` carries observations pre-signed for case `INC-4411`. Live: the
deploy branch passed `verify` on DeployBot's observation (shown as
`attested by DeployBot: errors.stopped @ …`); the security branch was
unsure at `Rotate` despite SecOps' observation, so the join was
`incomplete_join`. The same goal without observations never reaches
`Mitigated`. A join is labelled `completion attested` only when every
branch arrived attested. Live, on "5xx errors
since the deploy, and separately an API key was posted": the deploy
branch reached `Mitigated` and waited 5.3 s; the security branch stopped
at `Rotate` (unsure the old key was confirmed dead); the join escalated
`incomplete_join` instead of reporting the incident mitigated.

## What it does not do yet

- No actions are taken; walks model the investigation, not remediation.
- Proposals are provisional (M2). Some point back into symptoms
  (`Database -> Latency`) and would need rejecting.
- Intersection is structural (shared nodes). "Two agents about to restart
  the same service" needs resource-level claims, a later milestone.
