#!/usr/bin/env bash
# E1 runs: three labelled sets, three repeats each, Jev live, closed world
# (nothing learned), mock proposer (no proposal spend). From this folder:
#   ./run.sh            (needs TYPESAFE_API_KEY; builds nothing: uses ../../target/release/onto)
set -euo pipefail
cd "$(dirname "$0")"
onto=../../target/release/onto
R=runs; mkdir -p $R
common=(--closed-world --mock-proposer --no-memory)
for rep in 1 2 3; do
  $onto run data/large-flat.onto --jobs ../../demos/support-commons/large.jobs "${common[@]}" \
    --gaps $R/gaps.jsonl --telemetry $R/support-large.$rep.t.jsonl --dispositions $R/support-large.$rep.d.jsonl > $R/support-large.$rep.log
  $onto ensemble ../../demos/incident-response/perspectives.onto#Outage --jobs ../../demos/incident-response/perspectives.jobs \
    --closed-world --mock-proposer --gaps $R/gaps.jsonl --telemetry $R/incident-perspectives.$rep.t.jsonl --dispositions $R/incident-perspectives.$rep.d.jsonl > $R/incident-perspectives.$rep.log
  $onto ensemble ../../demos/hospital-discharge/perspectives.onto#WithThePatient --jobs ../../demos/hospital-discharge/perspectives.jobs \
    --closed-world --mock-proposer --gaps $R/gaps.jsonl --telemetry $R/discharge-perspectives.$rep.t.jsonl --dispositions $R/discharge-perspectives.$rep.d.jsonl > $R/discharge-perspectives.$rep.log
done
rm -f $R/gaps*.jsonl
