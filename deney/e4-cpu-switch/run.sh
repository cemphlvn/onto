#!/usr/bin/env bash
# E4 Jev passes: the test sets (baseline) and the training messages (teacher).
# Closed world, mock proposer. From this folder:  ./run.sh
set -euo pipefail
cd "$(dirname "$0")"
onto=../../target/release/onto
for job in test.message:Message test.ledger:Ledger test.events:Events train.message:Message; do
  f=${job%%:*}; cat=${job##*:}
  $onto run data/perspectives.onto#$cat --jobs data/$f.jobs --closed-world --mock-proposer --no-memory \
    --gaps runs/gaps.jsonl --telemetry runs/$f.t.jsonl --dispositions runs/$f.d.jsonl > runs/$f.log
  echo "$f: $(grep -c '' runs/$f.d.jsonl) records"
done
rm -f runs/gaps*.jsonl
