#!/usr/bin/env bash
# E2 run: every case judged twice, independently (which intent of 40, which
# team of 7), live Jev, closed world, mock proposer. From this folder:
#   ./run.sh      (needs TYPESAFE_API_KEY; uses ../../target/release/onto)
set -euo pipefail
cd "$(dirname "$0")"
../../target/release/onto ensemble data/boundary.onto#TeamLoop --jobs data/cases.jobs \
  --closed-world --mock-proposer --gaps runs/gaps.jsonl \
  --telemetry runs/teamloop.t.jsonl --dispositions runs/teamloop.d.jsonl --report runs/teamloop.report.json \
  > runs/teamloop.log
rm -f runs/gaps*.jsonl
tail -1 runs/teamloop.log
