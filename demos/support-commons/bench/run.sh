#!/bin/sh
# One judge on one language: bench/run.sh <tr|en> <judge-model> [onto binary]
# Closed world, no memory, mock proposer: only the judge is measured.
# Writes results/<lang>-<judge>.{dispositions.jsonl,json}.
set -eu
lang=$1 judge=$2 onto=${3:-onto}
here=$(cd "$(dirname "$0")" && pwd)
out="$here/results"; mkdir -p "$out"
tag="$lang-$(printf %s "$judge" | tr ':/' '__')"
start=$(python3 -c 'import time; print(time.time())')
"$onto" run "$here/flat-$lang.onto" --jobs "$here/$lang.jobs" --judge-model "$judge" \
  --closed-world --no-memory --mock-proposer --learned "$out/$tag.learned.jsonl" \
  --dispositions "$out/$tag.dispositions.jsonl" >"$out/$tag.log" 2>&1
wall=$(python3 -c "import time; print(round(time.time() - $start, 2))")
python3 "$here/score.py" "$out/$tag.dispositions.jsonl" "$wall" >"$out/$tag.json"
python3 -c "import json; d=json.load(open('$out/$tag.json')); print({k: d[k] for k in d if k != 'per_ticket'})"
