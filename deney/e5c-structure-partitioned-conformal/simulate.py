#!/usr/bin/env python3
"""E5c: structure-partitioned conformal deferral, factor by factor (MEASUREMENT.md).

Streams and recorded Jev judgments from ../e5b-novelty-signal (primary) and
E5's (secondary); learner code from ../e4-cpu-switch. No model calls.

    python3 simulate.py        # results/e5c.json
"""
import importlib.util
import itertools
import json
import sys
from multiprocessing import Pool
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE / "../e4-cpu-switch"))
import learn as E  # noqa: E402

spec = importlib.util.spec_from_file_location("e5b", HERE / "../e5b-novelty-signal/simulate.py")
B = importlib.util.module_from_spec(spec)
spec.loader.exec_module(B)

ORDERS = list(range(10))
RETRAIN, FIXED_GATE, JEV_GATE = 30, 0.8, 0.6
ALPHA, CAL_SHARE = 0.02, 0.40
N_MIN = int(np.ceil(1 / ALPHA - 1))  # 49: (0 + 1) / (n + 1) <= alpha
WINDOW = 30


def train(texts, targets):
    X = E.featurize(texts)
    W, b = E.fit_softmax(X, np.array(targets), 1e-4)
    S = (X @ X.T).toarray()
    np.fill_diagonal(S, -1)
    loo = np.sort(S.max(1))
    return {"W": W, "b": b, "T": 1.0, "X": X, "loo": loo}


def scores(m, texts, N):
    x = E.featurize(texts)
    P = E.softmax((x @ m["W"] + m["b"]) / m["T"])
    top, pred = P.max(1), P.argmax(1)
    if N:
        sim = (m["X"] @ x.T).toarray().max(0)
        rank = np.searchsorted(m["loo"], sim, side="right") / len(m["loo"])
        s = np.minimum(top, rank)
    else:
        s = top
    return s, pred


def conformal_lambda(s, wrong):
    n = len(s)
    if n < N_MIN:
        return np.inf
    for lam in np.concatenate([np.sort(np.unique(s)), [np.inf]]):
        risk = (np.sum((s >= lam) & wrong) + 1) / (n + 1)
        if risk <= ALPHA:
            return lam
    return np.inf


def run(args):
    stream_name, order, C, G, N, S, oracle = args
    cases, labels, first_n = B.primary() if stream_name == "primary" else B.secondary()
    s = B.order(cases, first_n, order)
    k = len(labels)
    eye = np.eye(k)
    rng = np.random.default_rng(5000 + order)
    L = []  # labels: {text, target, label, cal (random assignment), idx}
    seen_classes, change_point = set(), 0
    discovered, latency = set(), {}
    late_classes = sorted({c["y"] for c in s if c["late"]})
    model, lam = None, np.inf
    log = []
    first_late = next(i for i, c in enumerate(s) if c["late"])
    for i, c in enumerate(s):
        if i and i % RETRAIN == 0 and len(L) >= 20:
            if C:  # exchangeable: a random share held out
                cal = [l for l in L if l["cal"]]
                tr = [l for l in L if not l["cal"]]
            else:  # position-dependent: the most recent share held out
                cut = int(len(L) * (1 - CAL_SHARE))
                tr, cal = L[:cut], L[cut:]
            if S:
                cal = [l for l in cal if l["idx"] >= change_point]
            if len(tr) >= 10:
                model = train([l["text"] for l in tr], [l["target"] for l in tr])
                if len(cal) >= 5:
                    Z = E.featurize([l["text"] for l in cal]) @ model["W"] + model["b"]
                    model["T"] = E.fit_temperature(Z, eye[[l["label"] for l in cal]])
                if G and cal:
                    sc, pr = scores(model, [l["text"] for l in cal], N)
                    lam = conformal_lambda(sc, pr != np.array([l["label"] for l in cal]))
                elif G:
                    lam = np.inf
        by = None
        if model is not None:
            sc, pr = scores(model, [c["text"]], N)
            gate = lam if G else FIXED_GATE
            if sc[0] >= gate:
                by, ans = "cpu", int(pr[0])
        proposer = 0
        if by is None:
            if c["late"] and c["y"] not in discovered:
                discovered.add(c["y"])  # a gap: the open world learns the arrow
                proposer = 1
            jev_sure = c["p"].max() >= JEV_GATE
            by = "jev" if jev_sure else "person"
            ans = int(c["p"].argmax()) if jev_sure else c["y"]
            target = eye[c["y"]] if (oracle or not jev_sure) else c["p"]
            label = c["y"] if (oracle or not jev_sure) else int(c["p"].argmax())
            if label not in seen_classes:
                seen_classes.add(label)
                change_point = len(L)
            L.append({"text": c["text"], "target": target, "label": label, "cal": rng.random() < CAL_SHARE, "idx": len(L)})
        elif c["late"] and c["y"] not in discovered:
            latency[c["y"]] = latency.get(c["y"], 0) + 1
            ans = -1  # its arrow does not exist yet: wrong by construction
        log.append({"late": c["late"], "by": by, "right": ans == c["y"], "proposer": proposer})
    n = len(log)
    jev_calls = sum(r["by"] == "jev" for r in log)
    cpu = [r for r in log if r["by"] == "cpu"]
    wins = []
    for w in range(0, n, WINDOW):
        seg = [r for r in log[w:w + WINDOW] if r["by"] == "cpu"]
        wins.append(None if not seg else float(np.mean([not r["right"] for r in seg])))

    def err(a, b):
        seg = [r for r in log[max(a, 0):b] if r["by"] == "cpu"]
        return float(np.mean([not r["right"] for r in seg])) if seg else None

    return {
        "stream": stream_name, "order": order, "C": C, "G": G, "N": N, "S": S, "oracle": oracle,
        "accuracy": float(np.mean([r["right"] for r in log])),
        "per_call": n / max(jev_calls, 1), "proposer_calls": sum(r["proposer"] for r in log),
        "person": sum(r["by"] == "person" for r in log),
        "cpu_share": len(cpu) / n, "realized_risk": sum(not r["right"] for r in cpu) / n,
        "cpu_error_rate": float(np.mean([not r["right"] for r in cpu])) if cpu else None,
        "windows": wins, "err_before_novelty": err(first_late - 60, first_late),
        "err_after_novelty": err(first_late, first_late + 60),
        "latency": {labels[y]: latency.get(y, 0) for y in late_classes},
    }


def slope(wins):
    pts = [(i, w) for i, w in enumerate(wins) if w is not None]
    if len(pts) < 3:
        return None
    x = np.array([p[0] for p in pts], float); y = np.array([p[1] for p in pts])
    return float(np.polyfit(x, y, 1)[0])


def summarize(rs):
    def agg(key):
        v = [r[key] for r in rs if r[key] is not None]
        return {"mean": round(float(np.mean(v)), 4), "min": round(float(np.min(v)), 4), "max": round(float(np.max(v)), 4)} if v else None
    slopes = [slope(r["windows"]) for r in rs]
    slopes = [x for x in slopes if x is not None]
    boot = np.random.default_rng(0)
    bs = sorted(float(np.mean(boot.choice(slopes, len(slopes)))) for _ in range(2000)) if slopes else []
    lat = {}
    for r in rs:
        for k, v in r["latency"].items():
            lat.setdefault(k, []).append(v)
    return {**{k: agg(k) for k in ("accuracy", "per_call", "realized_risk", "cpu_share", "cpu_error_rate",
                                   "person", "proposer_calls", "err_before_novelty", "err_after_novelty")},
            "error_rate_slope_per_window": round(float(np.mean(slopes)), 5) if slopes else None,
            "slope_95": [round(bs[50], 5), round(bs[1949], 5)] if bs else None,
            "discovery_latency": {k: round(float(np.mean(v)), 2) for k, v in lat.items()}}


def main():
    jobs = [("primary", o, C, G, N, S, False) for C, G, N, S in itertools.product((0, 1), repeat=4) for o in ORDERS]
    jobs += [("primary", o, 1, 1, 1, 1, True) for o in ORDERS]                       # oracle teacher
    jobs += [("secondary", o, *cell, False) for cell in ((0, 0, 0, 0), (1, 1, 1, 1)) for o in ORDERS]
    with Pool(8) as pool:
        results = pool.map(run, jobs)
    cells = {}
    for r in results:
        key = f"{r['stream']} C{r['C']}G{r['G']}N{r['N']}S{r['S']}" + (" oracle" if r["oracle"] else "")
        cells.setdefault(key, []).append(r)
    out = {"alpha": ALPHA, "cal_share": CAL_SHARE, "n_min": N_MIN, "cells": {k: summarize(v) for k, v in cells.items()}}
    prim = {k: v for k, v in out["cells"].items() if k.startswith("primary C") and "oracle" not in k}
    eff = {}
    for f in "CGNS":
        on = [v for k, v in prim.items() if f"{f}1" in k]
        off = [v for k, v in prim.items() if f"{f}0" in k]
        eff[f] = {m: round(float(np.mean([x[m]["mean"] for x in on]) - np.mean([x[m]["mean"] for x in off])), 4)
                  for m in ("realized_risk", "per_call", "accuracy")}
    out["main_effects_primary"] = eff
    (HERE / "results/e5c.json").write_text(json.dumps(out, indent=1) + "\n")
    for k, v in out["cells"].items():
        print(f"{k:<28} acc {v['accuracy']['mean']:.3f} per_call {v['per_call']['mean']:.2f} risk {v['realized_risk']['mean']:.4f} "
              f"share {v['cpu_share']['mean']:.2f} slope {v['error_rate_slope_per_window']} {v['slope_95']} "
              f"err before/after {v['err_before_novelty'] and v['err_before_novelty']['mean']}/{v['err_after_novelty'] and v['err_after_novelty']['mean']} "
              f"latency {v['discovery_latency']}")
    print("main effects (primary):", eff)


if __name__ == "__main__":
    main()
