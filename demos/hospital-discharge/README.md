# Hospital Discharge

**A patient leaves hospital only when every part of going home is in
place, and a clinician signs off.**

**Human problem.** Discharges are delayed or unsafe because one part of
going home is missing: medicines not reconciled, no way to get home, no
support there, no follow-up. Each part belongs to a different team; the
patient waits in a bed, or goes home without what they need.

**Beneficiary.** The *patient leaving hospital* (and whoever cares for
them at home): four lines of work must all finish; when one does not,
the case says which.

## How it works

```
Discharge (split: four lines at once, no judgment)
  ├─ Medication ─ reconciled  attested medication.reconciled (Pharmacy) ─┐
  ├─ Transport  (choice, judged: family car | taxi | patient transport) ─┤
  ├─ HomeCare   (choice, judged: none | care visits | equipment)        ─┼─▶ ReadyToLeave  join: all
  └─ FollowUp   ─ booked                                               ─┘
ReadyToLeave ─▶ SignOff ─ signed_off  attested discharge.signed (Clinician)  ensures DischargeGrant
Discharged   entry: needs DischargeGrant
```

`state { case: summary, home, mobility; }` and `invariant unseen:
case.patient`: no model sees who the patient is. `admission HomeCare:
open_world`: new kinds of home support may be learned; everything else
is sealed.

```sh
onto run demos/hospital-discharge/discharge.onto --jobs demos/hospital-discharge/jobs \
    --policy shared --max-branches 6 --telemetry t.jsonl --dispositions d.jsonl
onto raster t.jsonl --dispositions d.jsonl --category demos/hospital-discharge/discharge.onto
```

## Observed (2026-09-24, live)

Built as the fork/join test of `docs/07-raster-findings.md` §4. No
discharge completed; the raster showed which line held each one up:
transport for three of four patients (none of the three options fit
"lives alone", "lives with his daughter; no car"), care at home for the
fourth (which learned `mobility_support` and stopped at the new frame).
The demo was designed with care at home as the gap; transport was the
real one.

## What it does not do

It does not book transport or care, or decide fitness for discharge; it
checks that the parts of going home are in place and that a clinician
has signed. Not clinical advice: the category encodes one fictional
ward's policy.
