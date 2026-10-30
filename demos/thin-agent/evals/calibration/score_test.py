"""Score the sealed test once (PREREGISTRATION.md): the map named in the
committed choice.json, fitted on all training records, on the test
systems' records; then every other candidate, labelled post hoc.

    uv run --with numpy --with scikit-learn python score_test.py
"""

import json
from collections import defaultdict
from pathlib import Path

import numpy as np

import study

HERE = Path(__file__).parent


def load(systems):
    rows = []
    for rec in sorted(study.RECORDS.glob("*.records.jsonl")):
        run = rec.name.removesuffix(".records.jsonl")
        system = run.split("-")[0]
        if system not in systems:
            continue
        gold = {json.loads(l)["object"]: json.loads(l)["target"]
                for l in (study.RECORDS / f"{run}.gold.jsonl").read_text().splitlines()}
        for line in rec.read_text().splitlines():
            r = json.loads(line)
            if r["p"] is None or r["object"] not in gold or r["target"] is None:
                continue
            c = r["candidates"]
            rows.append((system, run, r["p"], c[1]["p"] if len(c) > 1 else 0.0,
                         r["none"] or 0.0, r["target"] == gold[r["object"]]))
    return rows


def main():
    choice = json.loads((HERE / "choice.json").read_text())
    train = load(set(study.TRAIN_SYSTEMS))
    test_systems = sorted({p.name.split("-")[0] for p in study.RECORDS.glob("*.records.jsonl")}
                          - set(study.TRAIN_SYSTEMS))
    test = load(set(test_systems))
    col = lambda rows, k: np.array([r[k] for r in rows], dtype=float)
    print(f"train {len(train)} decisions; sealed test {len(test)} decisions from {', '.join(test_systems)}")
    print(f"test share right (raw judge): {col(test, 5).mean():.3f}\n")
    results = {}
    for name, fit in study.CANDIDATES:
        model = fit(col(train, 2), col(train, 3), col(train, 4), col(train, 5))
        q = np.asarray(model(col(test, 2), col(test, 3), col(test, 4)), dtype=float)
        all_ = study.scores(q, col(test, 5))
        per = {}
        for s in test_systems:
            m = np.array([r[0] == s for r in test])
            per[s] = round(study.scores(q[m], col(test, 5)[m])["brier"], 4)
        results[name] = dict(all_, per_system=per)
    pick = choice["chosen"]
    r = results[pick]
    print(f"PRE-REGISTERED CHOICE: {pick}")
    print(f"  Brier {r['brier']:.4f}  logloss {r['logloss']:.4f}  ECE {r['ece']:.3f}  "
          f"coverage@.9 {r['coverage']:.3f}  precision@.9 {r['precision']:.3f}  asked {r['asked']}")
    print(f"  per system Brier {r['per_system']}\n")
    print("post hoc (not used for any choice):")
    print(f"{'candidate':16} {'Brier':>7} {'logloss':>8} {'ECE':>6} {'cov@.9':>7} {'prec@.9':>8} {'asked':>6}")
    for name, _ in study.CANDIDATES:
        r = results[name]
        print(f"{name:16} {r['brier']:7.4f} {r['logloss']:8.4f} {r['ece']:6.3f} {r['coverage']:7.3f} "
              f"{r['precision']:8.3f} {r['asked']:6d}{'  <- pre-registered' if name == pick else ''}")
    (HERE / "test_result.json").write_text(json.dumps(
        dict(choice=pick, test_systems=test_systems, decisions=len(test), results=results), indent=1) + "\n")


if __name__ == "__main__":
    main()
