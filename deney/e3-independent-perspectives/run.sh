#!/usr/bin/env bash
# E3 run for one condition (ind, cor or pilot): three columns, live Jev, the
# columns see only their own channel. From this folder:  ./run.sh ind
set -euo pipefail
cd "$(dirname "$0")"
c=$1
../../target/release/onto ensemble data/perspectives.onto#Perspectives --jobs data/cases.$c.jobs \
  --closed-world --mock-proposer --gaps runs/gaps.jsonl \
  --telemetry runs/$c.t.jsonl --dispositions runs/$c.d.jsonl --report runs/$c.report.json > runs/$c.log
rm -f runs/gaps*.jsonl
grep -c "" runs/$c.d.jsonl | sed "s/^/frame records: /"
