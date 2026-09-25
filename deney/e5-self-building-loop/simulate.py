#!/usr/bin/env python3
"""E5: the self-building loop against Jev alone, on E4's recorded stream.

Uses E4's recorded Jev judgments and E4's learner code (imported from
../e4-cpu-switch, same branch lineage). No model calls. From this folder:

    python3 simulate.py            # the pre-registered loop: results/e5.json
    python3 simulate.py guard      # exploratory: the guarded loop, results/e5.guard.json
"""
import json
import sys
from pathlib import Path

import numpy as np
import scipy.optimize as opt

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE / "../e4-cpu-switch"))
import learn as E  # noqa: E402

LATE = ["wrong_item", "account_takeover"]
ORDERS = range(10)
RETRAIN = 30
CPU_GATE, JEV_GATE = 0.8, 0.6
WINDOW = 30
GUARD = len(sys.argv) > 1 and sys.argv[1] == "guard"
MIN_LABELS, AUDIT, AUDIT_WINDOW, AUDIT_AGREE, NEW_MIN = 90, 0.10, 20, 0.9, 10


def stream(seed):
    truth = E.load_truth()
    cases = []
    for split in ("train", "test"):
        jobs = E.read_jobs(f"{split}.message")
        J = E.jev(f"{split}.message")
        for cid, c in jobs.items():
            t = truth[cid]["truth"] or "none"
            cases.append({"id": cid, "text": E.text_of(c, "message"), "y": E.L[t], "late": t in LATE, "p": J[cid][0]})
    rng = np.random.default_rng(seed)
    early = [c for c in cases if not c["late"]]
    late = [c for c in cases if c["late"]]
    rng.shuffle(early)
    first, rest = early[:270], early[270:] + late
    rng.shuffle(rest)
    return first + rest


def train(texts, Y):
    X = E.featurize(texts)
    n = len(texts)
    cut = int(n * 0.75)
    W, b = E.fit_softmax(X[:cut], Y[:cut], 1e-4)
    T = E.fit_temperature(X[cut:] @ W + b, Y[cut:]) if n - cut >= 5 else 1.0
    return {"W": W, "b": b, "T": T}


def run(order):
    s = stream(order)
    eye = np.eye(len(E.LABELS))
    texts, targets, model = [], [], None
    known, audits, suspended = set(), [], set()
    rng = np.random.default_rng(1000 + order)
    log = []
    for i, c in enumerate(s):
        if i and i % RETRAIN == 0 and len(texts) >= 20:
            model = train(texts, np.array(targets))
            counts = np.bincount([int(np.argmax(t)) for t in targets], minlength=len(E.LABELS))
            known = {k for k in range(len(E.LABELS)) if counts[k] > 0}
            suspended = {k for k in suspended if counts[k] < NEW_MIN}
        by = None
        if model is not None:
            P = E.predict_closed(model, [c["text"]])[0]
            ok = P.max() >= CPU_GATE
            if GUARD:
                recent = audits[-AUDIT_WINDOW:]
                ok = ok and len(texts) >= MIN_LABELS and not suspended and \
                    (len(recent) < 5 or np.mean(recent) >= AUDIT_AGREE)
                if ok and rng.random() < AUDIT:
                    # Audit: Jev judges this case too (a model call); its answer is used.
                    j = int(c["p"].argmax())
                    audits.append(j == int(P.argmax()))
                    if j not in known:
                        suspended.add(j)
                    ok = False
            if ok:
                by, ans = "cpu", int(P.argmax())
        if by is None:
            if c["p"].max() >= JEV_GATE:
                by, ans = "jev", int(c["p"].argmax())
                texts.append(c["text"]); targets.append(c["p"])
            else:
                by, ans = "person", c["y"]
                texts.append(c["text"]); targets.append(eye[c["y"]])
        # Jev alone on the same case.
        base_by = "jev" if c["p"].max() >= JEV_GATE else "person"
        base_ans = int(c["p"].argmax()) if base_by == "jev" else c["y"]
        log.append({"i": i, "late": c["late"], "by": by, "right": ans == c["y"],
                    "base_by": base_by, "base_right": base_ans == c["y"]})
    return log


def fit_growth(n, s):
    """Saturating exponential vs power law, least squares."""
    def sse(f, p):
        return float(((f(n, *p) - s) ** 2).sum())
    fexp = lambda n, a, tau: a * (1 - np.exp(-n / tau))
    fpow = lambda n, a, b: a * np.power(n, b)
    pe, _ = opt.curve_fit(fexp, n, s, p0=[0.8, 60], bounds=([0, 1], [1, 5000]), maxfev=20000)
    pp, _ = opt.curve_fit(fpow, n, s, p0=[0.1, 0.5], bounds=([0, 0], [10, 3]), maxfev=20000)
    return {"exponential": {"s_inf": round(pe[0], 3), "tau_cases": round(pe[1], 1), "sse": round(sse(fexp, pe), 4)},
            "power": {"a": round(pp[0], 4), "b": round(pp[1], 3), "sse": round(sse(fpow, pp), 4)}}


def main():
    runs = [run(o) for o in ORDERS]
    per = []
    for log in runs:
        n = len(log)
        calls = sum(r["by"] == "jev" for r in log)
        base_calls = n  # Jev alone calls Jev on every case
        per.append({
            "resolved_per_call": n / max(calls, 1), "base_resolved_per_call": n / base_calls,
            "accuracy": np.mean([r["right"] for r in log]), "base_accuracy": np.mean([r["base_right"] for r in log]),
            "person": sum(r["by"] == "person" for r in log), "base_person": sum(r["base_by"] == "person" for r in log),
            "cpu_share": np.mean([r["by"] == "cpu" for r in log]), "jev_calls": calls,
        })
    agg = lambda k: {"mean": round(float(np.mean([p[k] for p in per])), 3),
                     "min": round(float(np.min([p[k] for p in per])), 3), "max": round(float(np.max([p[k] for p in per])), 3)}
    out = {k: agg(k) for k in per[0]}

    # Growth curve: learner share per window over the first phase (270 cases), mean over orders.
    windows = list(range(0, 540, WINDOW))
    curve = [float(np.mean([np.mean([r["by"] == "cpu" for r in log[w:w + WINDOW]]) for log in runs])) for w in windows]
    errs = {}
    for log in runs:
        for r in log:
            if not r["right"]:
                k = f"{r['by']} on {'late' if r['late'] else 'early'} situations"
                errs[k] = errs.get(k, 0) + 1
    out["errors_by_source_total_over_orders"] = errs
    out["cpu_share_by_window"] = [{"cases_seen": w, "cpu_share": round(c, 3)} for w, c in zip(windows, curve)]
    first = [(w + WINDOW / 2, c) for w, c in zip(windows, curve) if w < 270]
    out["growth_fit_first_phase"] = fit_growth(np.array([x for x, _ in first]), np.array([y for _, y in first]))

    # Take-over of the late situations, by how many of their cases have been seen.
    buckets = {}
    for log in runs:
        k = 0
        for r in log:
            if r["late"]:
                b = min(k // 10, 7)
                buckets.setdefault(b, []).append(r["by"] == "cpu")
                k += 1
    out["late_situations_cpu_share"] = [{"their_cases_seen": f"{b * 10}-{b * 10 + 9}" if b < 7 else "70+",
                                         "cpu_share": round(float(np.mean(v)), 3), "n": len(v)}
                                        for b, v in sorted(buckets.items())]
    (HERE / "results").mkdir(exist_ok=True)
    out["variant"] = "guarded (exploratory)" if GUARD else "pre-registered"
    (HERE / ("results/e5.guard.json" if GUARD else "results/e5.json")).write_text(json.dumps(out, indent=1) + "\n")
    print(json.dumps(out, indent=1))


if __name__ == "__main__":
    main()
