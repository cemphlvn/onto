"""Wall-clock time of the same decisions with onto and without it. The
decision records carry tokens but no time, so time is measured here on a
fixed slice (Lucene and TEAMMATES, names visible: 57 packages), each arm
at one request in flight (latency) and at sixteen (throughput, onto's
default --concurrency).

    python3 timing.py MODEL [--arms=onto-old,onto,direct] [--conc=1,16]

Arms:
  onto-old  onto before brief frames and the remembered log-probability
            probe (the binary at $ONTO_OLD, default
            target-local/onto-before-trim): full frames, two requests.
  onto      onto discover --brief with --judge-model MODEL on the category
            files the sysmdl runs generated; records go to a scratch
            directory, never over the leaderboard's.
  direct    direct_llm.py's prompt, one request per package, MODEL via
            OpenRouter; per-call latency is recorded too.

Writes results/timing-<model>.json: per arm and concurrency, seconds,
decisions, failed, accuracy against the gold files, tokens and requests.
"""

import json
import os
import re
import subprocess
import sys
import tempfile
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

import direct_llm as direct

HERE = Path(__file__).parent
EVALS = HERE.parent
ONTO = os.environ.get("ONTO_BIN", str(HERE.parents[3] / "target/release/onto"))
ONTO_OLD = os.environ.get("ONTO_OLD", str(HERE.parents[3] / "target-local/onto-before-trim"))
sys.path.insert(0, str(EVALS / "sysmdl"))
from sysmdl_eval import FRAMING  # noqa: E402 - the onto runs' own framing

SLICE = [("Lucene", "Lucene-visible-bp-doc"), ("Teammates", "Teammates-visible-bp-doc")]


def gold(tag):
    return {json.loads(l)["object"]: json.loads(l)["target"]
            for l in (direct.RECORDS / f"{tag}.gold.jsonl").read_text().splitlines()}


def score(recs, truth):
    right = sum(1 for r in recs if r["disposition"] == "adopted" and r["target"] == truth.get(r["object"]))
    return right, sum(r["disposition"] == "failed" for r in recs)


def onto_arm(model, conc, old=False):
    out = []
    with tempfile.TemporaryDirectory() as tmp:
        for name, tag in SLICE:
            path = EVALS / "sysmdl" / f"l2-{tag}.onto"
            rec = Path(tmp) / f"{tag}.records.jsonl"
            t0 = time.monotonic()
            r = subprocess.run(
                [ONTO_OLD if old else ONTO, "discover", f"{path}#{name}Code", f"{path}#Intended", "--non-injective", "--no-roles",
                 "--name", "Proposed", "--goal", FRAMING["goal"], "--instructions", FRAMING["instructions"],
                 "--records", str(rec), "--judge-model", f"openrouter:{model}", "--concurrency", str(conc)]
                + ([] if old else ["--brief"]),
                capture_output=True, text=True, timeout=7200)
            secs = time.monotonic() - t0
            if r.returncode:
                sys.exit(f"onto {tag} failed:\n{(r.stdout + r.stderr)[-1500:]}")
            recs = [json.loads(l) for l in rec.read_text().splitlines()]
            out.append((tag, secs, recs, None))
    return out


def direct_arm(model, conc):
    out = []
    for _, tag in SLICE:
        about, deps, modules, mabout = direct.parse(EVALS / "sysmdl" / f"l2-{tag}.onto")
        objs = list(about)

        def timed(obj):
            t0 = time.monotonic()
            [rec] = direct.one(model, EVALS / "sysmdl" / f"l2-{tag}.onto", tag, only={obj})
            rec["seconds"] = time.monotonic() - t0
            return rec

        t0 = time.monotonic()
        with ThreadPoolExecutor(max_workers=conc) as pool:
            recs = list(pool.map(timed, objs))
        out.append((tag, time.monotonic() - t0, recs, [r["seconds"] for r in recs]))
    return out


def summarize(arm, conc, runs):
    n = right = failed = tin = tout = reqs = 0
    secs = 0.0
    lat = []
    for tag, s, recs, latencies in runs:
        truth = gold(tag)
        recs = [r for r in recs if r["object"] in truth]
        a, f = score(recs, truth)
        n += len(recs); right += a; failed += f; secs += s
        for r in recs:
            t = r.get("tokens") or {}
            tin += t.get("input", 0); tout += t.get("output", 0); reqs += t.get("requests", 1)
        lat += latencies or []
    lat.sort()
    return {"arm": arm, "concurrency": conc, "decisions": n, "seconds": round(secs, 1),
            "seconds_per_decision": round(secs / n, 2), "accuracy": round(right / n, 3), "failed": failed,
            "input_tokens": tin, "output_tokens": tout, "requests": reqs,
            "latency_p50": round(lat[len(lat) // 2], 2) if lat else None,
            "latency_p90": round(lat[int(len(lat) * 0.9)], 2) if lat else None}


def main():
    model = sys.argv[1]
    opt = lambda k, d: next((a.split("=", 1)[1] for a in sys.argv[2:] if a.startswith(f"--{k}=")), d)
    arms = opt("arms", "onto-old,onto,direct").split(",")
    concs = [int(c) for c in opt("conc", "1,16").split(",")]
    dest = HERE / "results" / f"timing-{re.sub(r'[^A-Za-z0-9.]+', '_', model)}.json"
    dest.parent.mkdir(exist_ok=True)
    rows = json.loads(dest.read_text()) if dest.exists() else []
    for arm in arms:
        for conc in concs:
            runs = {"onto-old": lambda m, c: onto_arm(m, c, old=True), "onto": onto_arm,
                    "direct": direct_arm}[arm](model, conc)
            row = summarize(arm, conc, runs)
            rows = [r for r in rows if (r["arm"], r["concurrency"]) != (arm, conc)] + [row]
            dest.write_text(json.dumps(rows, indent=1) + "\n")
            print(json.dumps(row), flush=True)


if __name__ == "__main__":
    main()
