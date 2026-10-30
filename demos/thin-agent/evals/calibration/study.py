"""The calibration study of PREREGISTRATION.md: 12 candidate maps,
leave-one-system-out on the training records, the one-standard-error
rule on held-out Brier. Training records only: the sealed test is not
read here.

    uv run --with numpy --with scikit-learn python study.py [--choose]

--choose writes choice.json (the frozen choice) after printing.
"""

import json
import math
import sys
from collections import defaultdict
from pathlib import Path

import numpy as np
from sklearn.isotonic import IsotonicRegression
from sklearn.linear_model import LogisticRegression

HERE = Path(__file__).parent
RECORDS = HERE / "records"
TRAIN_SYSTEMS = ("JabRef", "Teammates", "Argouml")
EPS = 1e-6


def load():
    """(system, run, p, second, none, right) per judged decision."""
    rows = []
    for rec in sorted(RECORDS.glob("*.records.jsonl")):
        run = rec.name.removesuffix(".records.jsonl")
        system = run.split("-")[0]
        if system not in TRAIN_SYSTEMS:
            continue
        gold = {}
        for line in (RECORDS / f"{run}.gold.jsonl").read_text().splitlines():
            g = json.loads(line)
            gold[g["object"]] = g["target"]
        for line in rec.read_text().splitlines():
            r = json.loads(line)
            if r["p"] is None or r["object"] not in gold or r["target"] is None:
                continue
            c = r["candidates"]
            second = c[1]["p"] if len(c) > 1 else 0.0
            rows.append((system, run, r["p"], second, r["none"] or 0.0, r["target"] == gold[r["object"]]))
    return rows


def logit(p):
    p = np.clip(p, EPS, 1 - EPS)
    return np.log(p / (1 - p))


def fit_logistic(X, y, C):
    if len(set(y)) < 2:  # one class only: predict its rate
        rate = float(np.mean(y))
        return lambda Z: np.full(len(Z), rate)
    m = LogisticRegression(C=C, max_iter=1000).fit(X, y)
    return lambda Z: m.predict_proba(Z)[:, 1]


def platt_pull(p, y, pull):
    """Platt with a Gaussian prior of weight `pull` centred on identity,
    as onto-core's calibrate::Platt (damped Newton)."""
    x = logit(p)
    a, b = 1.0, 0.0

    def obj(a, b):
        z = a * x + b
        return np.sum(np.where(y, np.logaddexp(0, -z), np.logaddexp(0, z))) + 0.5 * pull * ((a - 1) ** 2 + b ** 2)

    for _ in range(100):
        q = 1 / (1 + np.exp(-(a * x + b)))
        r, s = q - y, q * (1 - q)
        ga, gb = np.sum(r * x) + pull * (a - 1), np.sum(r) + pull * b
        haa, hab, hbb = np.sum(s * x * x) + pull, np.sum(s * x), np.sum(s) + pull
        det = haa * hbb - hab * hab
        if abs(det) < 1e-12:
            break
        da, db = (hbb * ga - hab * gb) / det, (haa * gb - hab * ga) / det
        t, before = 1.0, obj(a, b)
        while obj(a - t * da, b - t * db) > before and t > 1e-6:
            t /= 2
        a, b = a - t * da, b - t * db
        if t * (abs(da) + abs(db)) < 1e-10:
            break
    return lambda q: 1 / (1 + np.exp(-(a * logit(q) + b)))


def temperature(p, y):
    x = logit(p)
    best = min(np.linspace(0.05, 5, 400),
               key=lambda a: np.sum(np.where(y, np.logaddexp(0, -a * x), np.logaddexp(0, a * x))))
    return lambda q: 1 / (1 + np.exp(-best * logit(q)))


def binning(p, y, edges):
    idx = np.clip(np.searchsorted(edges, p, side="right") - 1, 0, len(edges) - 2)
    prior = float(np.mean(y))
    rate = [float(np.mean(y[idx == k])) if np.any(idx == k) else prior for k in range(len(edges) - 1)]
    return lambda q: np.array(rate)[np.clip(np.searchsorted(edges, q, side="right") - 1, 0, len(edges) - 2)]


def isotonic(p, y, interpolate):
    iso = IsotonicRegression(out_of_bounds="clip", y_min=0, y_max=1).fit(p, y)
    if not interpolate:
        # Step function: the value of the nearest fitted threshold below.
        xs, ys = iso.X_thresholds_, iso.y_thresholds_
        return lambda q: ys[np.clip(np.searchsorted(xs, q, side="right") - 1, 0, len(xs) - 1)]
    return lambda q: iso.predict(q)


def venn_abers(p, y):
    def predict(q):
        out = []
        for v in q:
            pr = []
            for label in (0, 1):
                iso = IsotonicRegression(out_of_bounds="clip", y_min=0, y_max=1).fit(
                    np.append(p, v), np.append(y, label))
                pr.append(float(iso.predict([v])[0]))
            out.append((pr[0] + pr[1]) / 2)
        return np.array(out)
    return predict


# name -> fit(p, second, none, y) -> predict(p, second, none); simplest first
CANDIDATES = [
    ("identity", lambda p, s, n, y: (lambda P, S, N: P)),
    ("temperature", lambda p, s, n, y: (lambda f: lambda P, S, N: f(P))(temperature(p, y))),
    ("platt_pull1", lambda p, s, n, y: (lambda f: lambda P, S, N: f(P))(platt_pull(p, y, 1.0))),
    ("platt", lambda p, s, n, y: (lambda f: lambda P, S, N: f(P))(platt_pull(p, y, 0.0))),
    ("platt_none", lambda p, s, n, y: (lambda f: lambda P, S, N: f(np.c_[logit(P), logit(N)]))(
        fit_logistic(np.c_[logit(p), logit(n)], y, 1e4))),
    ("platt_margin", lambda p, s, n, y: (lambda f: lambda P, S, N: f(np.c_[logit(P), P - S]))(
        fit_logistic(np.c_[logit(p), p - s], y, 1e4))),
    ("beta", lambda p, s, n, y: (lambda f: lambda P, S, N: f(np.c_[np.log(np.clip(P, EPS, 1)), -np.log(np.clip(1 - P, EPS, 1))]))(
        fit_logistic(np.c_[np.log(np.clip(p, EPS, 1)), -np.log(np.clip(1 - p, EPS, 1))], y, 1e4))),
    ("hist5_width", lambda p, s, n, y: (lambda f: lambda P, S, N: f(P))(binning(p, y, np.linspace(0, 1, 6)))),
    ("hist10_mass", lambda p, s, n, y: (lambda f: lambda P, S, N: f(P))(
        binning(p, y, np.unique(np.r_[0, np.quantile(p, np.linspace(0, 1, 11))[1:-1], 1.0000001])))),
    ("isotonic", lambda p, s, n, y: (lambda f: lambda P, S, N: f(P))(isotonic(p, y, False))),
    ("isotonic_interp", lambda p, s, n, y: (lambda f: lambda P, S, N: f(P))(isotonic(p, y, True))),
    ("venn_abers", lambda p, s, n, y: (lambda f: lambda P, S, N: f(P))(venn_abers(p, y))),
]


def scores(q, y, threshold=0.9):
    q = np.clip(q, EPS, 1 - EPS)
    brier = float(np.mean((q - y) ** 2))
    logloss = float(-np.mean(y * np.log(q) + (1 - y) * np.log(1 - q)))
    ece = 0.0
    for k in range(5):
        m = (q >= k / 5) & ((q < (k + 1) / 5) if k < 4 else (q <= 1))
        if m.any():
            ece += m.mean() * abs(y[m].mean() - q[m].mean())
    adopted = q >= threshold
    return dict(brier=brier, logloss=logloss, ece=ece, coverage=float(adopted.mean()),
                precision=float(y[adopted].mean()) if adopted.any() else float("nan"),
                asked=int((~adopted).sum()))


def main():
    rows = load()
    systems = sorted({r[0] for r in rows})
    print(f"{len(rows)} labelled decisions; " + ", ".join(
        f"{s} {sum(r[0] == s for r in rows)} ({np.mean([r[5] for r in rows if r[0] == s]):.2f} right)" for s in systems))
    arr = lambda k, sel: np.array([r[k] for r in rows if sel(r)], dtype=float)
    results = {}
    for name, fit in CANDIDATES:
        folds = []
        for held in systems:
            tr, te = (lambda r: r[0] != held), (lambda r: r[0] == held)
            model = fit(arr(2, tr), arr(3, tr), arr(4, tr), arr(5, tr))
            q = np.asarray(model(arr(2, te), arr(3, te), arr(4, te)), dtype=float)
            folds.append(scores(q, arr(5, te)))
        b = np.array([f["brier"] for f in folds])
        results[name] = dict(
            brier=float(b.mean()), se=float(b.std(ddof=1) / math.sqrt(len(b))),
            **{k: float(np.nanmean([f[k] for f in folds])) for k in ("logloss", "ece", "coverage", "precision")},
            asked=int(sum(f["asked"] for f in folds)), folds={s: round(f["brier"], 4) for s, f in zip(systems, folds)})
    best = min(results, key=lambda k: results[k]["brier"])
    bar = results[best]["brier"] + results[best]["se"]
    chosen = next(name for name, _ in CANDIDATES if results[name]["brier"] <= bar)
    print(f"\n{'candidate':16} {'Brier':>7} {'± SE':>6} {'logloss':>8} {'ECE':>6} {'cov@.9':>7} {'prec@.9':>8} {'asked':>6}   held-out Brier per system")
    for name, _ in CANDIDATES:
        r = results[name]
        mark = " <- chosen" if name == chosen else (" (best)" if name == best else "")
        print(f"{name:16} {r['brier']:7.4f} {r['se']:6.4f} {r['logloss']:8.4f} {r['ece']:6.3f} {r['coverage']:7.3f} {r['precision']:8.3f} {r['asked']:6d}   {r['folds']}{mark}")
    print(f"\nbest {best} {results[best]['brier']:.4f} + SE {results[best]['se']:.4f} = {bar:.4f}; "
          f"the simplest within it: {chosen}")
    if "--choose" in sys.argv:
        (HERE / "choice.json").write_text(json.dumps(
            dict(chosen=chosen, rule="one-standard-error on leave-one-system-out Brier",
                 train_systems=systems, decisions=len(rows), results=results), indent=1) + "\n")
        print("choice.json written: commit it before scoring the sealed test")


if __name__ == "__main__":
    main()
