#!/usr/bin/env python3
"""E4: CPU switch learners against Jev (MEASUREMENT.md). numpy + scipy.

python3 learn.py        # writes results/e4.json
"""
import json
import re
import time
import zlib
from collections import defaultdict
from pathlib import Path

import numpy as np
import scipy.optimize as opt
import scipy.sparse as sp

HERE = Path(__file__).resolve().parent
D = json.loads((HERE / "data/design.json").read_text())
H = D["features"]["hash_dim"]
E3 = json.loads((HERE / "../e3-independent-perspectives/data/design.json").read_text())
SIT = [c["id"] for c in E3["classes"]]
LABELS = SIT + ["none"]
L = {k: i for i, k in enumerate(LABELS)}
LAMBDAS = [1e-4, 1e-3, 1e-2]
rng0 = np.random.default_rng(0)


# ---------------------------------------------------------------- data
def load_truth():
    return json.loads((HERE / "data/truth.json").read_text())


def text_of(case, ch):
    v = case[ch]
    return v if isinstance(v, str) else json.dumps(v, sort_keys=True) if isinstance(v, dict) else " ".join(v)


def read_jobs(name):
    out = {}
    for line in open(HERE / f"data/{name}.jobs"):
        if line.startswith("Read: "):
            c = json.loads(line[6:])
            out[c["id"]] = c
    return out


def jev(name):
    """case id -> (probability over LABELS, latency ms, escalated)."""
    walks = {}
    for line in open(HERE / f"runs/{name}.t.jsonl"):
        e = json.loads(line)
        if e.get("event") == "walk.start" and e.get("case"):
            walks[e["walk"]] = e["case"]
    out = {}
    for line in open(HERE / f"runs/{name}.d.jsonl"):
        r = json.loads(line)
        if r["walk"] not in walks or r["at"] != "Read" or not r.get("judge"):
            continue
        p = np.zeros(len(LABELS))
        for c in r["candidates"]:
            if c.get("judgment") is not None:
                p[L[c["arrow"]] if c["arrow"] in L else L["none"]] += c["judgment"]
        p[L["none"]] += r["judge"].get("none_of_these") or 0.0
        p = p / p.sum() if p.sum() > 0 else np.full(len(LABELS), 1 / len(LABELS))
        out[walks[r["walk"]]] = (p, r["judge"]["latency_ms"], r["outcome"]["kind"] != "followed")
    return out


# ------------------------------------------------------------ features
def tokens(s):
    w = re.findall(r"[a-z0-9_]+", s.lower())
    return w + [a + " " + b for a, b in zip(w, w[1:])]


def h(t):
    return zlib.crc32(t.encode()) % H


def featurize(texts):
    rows, cols = [], []
    for i, s in enumerate(texts):
        idx = sorted({h(t) for t in tokens(s)})
        rows += [i] * len(idx)
        cols += idx
    X = sp.csr_matrix((np.ones(len(rows)), (rows, cols)), shape=(len(texts), H))
    norms = np.sqrt(np.asarray(X.multiply(X).sum(1)).ravel()) + 1e-9
    return sp.diags(1 / norms) @ X


# ------------------------------------------------------- closed model
def softmax(Z):
    Z = Z - Z.max(1, keepdims=True)
    E = np.exp(Z)
    return E / E.sum(1, keepdims=True)


def fit_softmax(X, Y, lam):
    """Y: rows of target distributions (one-hot for truth, soft for distillation)."""
    n, d = X.shape
    k = Y.shape[1]

    def f(w):
        W = w[:d * k].reshape(d, k)
        b = w[d * k:]
        P = softmax(X @ W + b)
        loss = -(Y * np.log(P + 1e-12)).sum() / n + lam / 2 * (W * W).sum()
        G = (P - Y) / n
        gW = X.T @ G + lam * W
        return loss, np.concatenate([np.asarray(gW).ravel(), G.sum(0)])

    w0 = np.zeros(d * k + k)
    r = opt.minimize(f, w0, jac=True, method="L-BFGS-B", options={"maxiter": 500})
    return r.x[:d * k].reshape(d, k), r.x[d * k:]


def fit_temperature(Z, Y):
    def nll(t):
        P = softmax(Z / t[0])
        return -(Y * np.log(P + 1e-12)).sum() / len(Y)
    return opt.minimize(nll, [1.0], bounds=[(0.05, 20)]).x[0]


def train_closed(texts, Y, seed=7):
    idx = np.random.default_rng(seed).permutation(len(texts))
    nv = int(len(texts) * D["split"]["validation_share_of_train"])
    va, tr = idx[:nv], idx[nv:]
    X = featurize(texts)
    best = None
    for lam in LAMBDAS:
        W, b = fit_softmax(X[tr], Y[tr], lam)
        P = softmax(X[va] @ W + b)
        ll = -(Y[va] * np.log(P + 1e-12)).sum() / len(va)
        if best is None or ll < best[0]:
            best = (ll, lam, W, b)
    _, lam, W, b = best
    T = fit_temperature(X[va] @ W + b, Y[va])
    return {"W": W, "b": b, "T": T, "lambda": lam}


def predict_closed(m, texts):
    return softmax((featurize(texts) @ m["W"] + m["b"]) / m["T"])


# --------------------------------------------------- described model
DESCR = {}
src = (HERE / "data/perspectives.onto").read_text()
block = src[src.index("category Message"):src.index("functor MessageView")]
for a, d in re.findall(r'    ([a-z_]+): Read -> \w+ "([^"]+)"', block):
    DESCR["none" if a == "no_signal" else a] = d


def pair_features(texts, options):
    rows, cols, vals = [], [], []
    r = 0
    for s in texts:
        ts = set(tokens(s))
        for o in options:
            ds = set(tokens(DESCR[o]))
            idx = {h("x|" + a + "|" + b) for a in ts for b in ds if " " not in a and " " not in b}
            idx.add(h("overlap"))
            for j in idx:
                rows.append(r); cols.append(j)
                vals.append(len(ts & ds) / (len(ds) + 1) if j == h("overlap") else 1.0)
            r += 1
    return sp.csr_matrix((vals, (rows, cols)), shape=(r, H))


def train_described(texts, labels, options):
    X = pair_features(texts, options)
    y = np.array([1.0 if labels[i] == o else 0.0 for i in range(len(texts)) for o in options])

    def f(w):
        z = X @ w[:-1] + w[-1]
        p = 1 / (1 + np.exp(-z))
        loss = -(y * np.log(p + 1e-12) + (1 - y) * np.log(1 - p + 1e-12)).mean() + 1e-3 / 2 * (w[:-1] ** 2).sum()
        g = p - y
        return loss, np.concatenate([X.T @ g / len(y) + 1e-3 * w[:-1], [g.mean()]])

    r = opt.minimize(f, np.zeros(H + 1), jac=True, method="L-BFGS-B", options={"maxiter": 500})
    return r.x


def predict_described(w, texts, options):
    z = (pair_features(texts, options) @ w[:-1] + w[-1]).reshape(len(texts), len(options))
    return softmax(z)


# ------------------------------------------------------------ measures
def ece_top(P, y):
    top = P.max(1)
    right = (P.argmax(1) == y).astype(float)
    bins = np.minimum((top * 10).astype(int), 9)
    return sum((bins == k).sum() / len(y) * abs(top[bins == k].mean() - right[bins == k].mean())
               for k in range(10) if (bins == k).any())


def coverage_at(P, y, target):
    order = np.argsort(-P.max(1))
    right = (P.argmax(1) == y)[order]
    acc = np.cumsum(right) / np.arange(1, len(y) + 1)
    ok = np.where(acc >= target - 1e-12)[0]
    return float((ok.max() + 1) / len(y)) if len(ok) else 0.0


def boot(P, y, fn, n=2000):
    vals = []
    for _ in range(n):
        i = rng0.integers(0, len(y), len(y))
        vals.append(fn(P[i], y[i]))
    vals.sort()
    return [round(float(vals[int(0.025 * n)]), 3), round(float(vals[int(0.975 * n) - 1]), 3)]


def report(P, y):
    acc = lambda P, y: float((P.argmax(1) == y).mean())
    return {"accuracy": round(acc(P, y), 3), "accuracy_95": boot(P, y, acc),
            "ece_top": round(ece_top(P, y), 3), "ece_top_95": boot(P, y, ece_top),
            "log_loss": round(float(-np.log(P[np.arange(len(y)), y] + 1e-12).mean()), 3),
            "mean_top_p": round(float(P.max(1).mean()), 3)}


# ---------------------------------------------------------------- main
def main():
    truth = load_truth()
    lab = lambda cid: L[truth[cid]["truth"] or "none"]
    out = {"labels": LABELS}

    for ch, cat in (("ledger", "Ledger"), ("events", "Events"), ("message", "Message")):
        train = read_jobs(f"train.{ch}")
        test = read_jobs(f"test.{ch}")
        tr_ids, te_ids = list(train), list(test)
        tr_text = [text_of(train[i], ch) for i in tr_ids]
        te_text = [text_of(test[i], ch) for i in te_ids]
        y_tr = np.array([lab(i) for i in tr_ids])
        y_te = np.array([lab(i) for i in te_ids])
        J = jev(f"test.{ch}")
        PJ = np.array([J[i][0] for i in te_ids])
        res = {"train": len(tr_ids), "test": len(te_ids),
               "jev": {**report(PJ, y_te), "escalated": int(sum(J[i][2] for i in te_ids)),
                       "latency_ms_mean": round(float(np.mean([J[i][1] for i in te_ids])), 1)}}
        m = train_closed(tr_text, np.eye(len(LABELS))[y_tr])
        t0 = time.perf_counter()
        P = predict_closed(m, te_text)
        per_case_ms = (time.perf_counter() - t0) * 1000 / len(te_ids)
        res["closed_truth"] = {**report(P, y_te), "lambda": m["lambda"], "temperature": round(float(m["T"]), 3),
                               "latency_ms_per_case": round(per_case_ms, 4),
                               "coverage_at_jev_accuracy": round(coverage_at(P, y_te, res["jev"]["accuracy"]), 3)}
        if ch == "message":
            JT = jev("train.message")
            Ysoft = np.array([JT[i][0] for i in tr_ids])
            md = train_closed(tr_text, Ysoft)
            Pd = predict_closed(md, te_text)
            res["closed_distill"] = {**report(Pd, y_te), "lambda": md["lambda"], "temperature": round(float(md["T"]), 3),
                                     "coverage_at_jev_accuracy": round(coverage_at(Pd, y_te, res["jev"]["accuracy"]), 3),
                                     "teacher_agreement_with_truth_on_train": round(float((Ysoft.argmax(1) == y_tr).mean()), 3)}
            # Open vocabulary: two situations held out entirely per fold.
            folds = []
            for held in D["open_vocabulary_folds"]:
                seen = [o for o in LABELS if o not in held]
                keep = [k for k, i in enumerate(tr_ids) if LABELS[y_tr[k]] not in held]
                w = train_described([tr_text[k] for k in keep], [LABELS[y_tr[k]] for k in keep], seen)
                Pa = predict_described(w, te_text, LABELS)
                is_held = np.array([LABELS[v] in held for v in y_te])
                folds.append({"held_out": held,
                              "accuracy_on_held_out": round(float((Pa[is_held].argmax(1) == y_te[is_held]).mean()), 3),
                              "accuracy_on_seen": round(float((Pa[~is_held].argmax(1) == y_te[~is_held]).mean()), 3),
                              "jev_on_held_out": round(float((PJ[is_held].argmax(1) == y_te[is_held]).mean()), 3)})
            res["described_open_vocabulary"] = {
                "folds": folds,
                "held_out_accuracy_mean": round(float(np.mean([f["accuracy_on_held_out"] for f in folds])), 3),
                "seen_accuracy_mean": round(float(np.mean([f["accuracy_on_seen"] for f in folds])), 3),
                "chance": round(1 / len(LABELS), 3)}
        out[ch] = res

    (HERE / "results").mkdir(exist_ok=True)
    (HERE / "results/e4.json").write_text(json.dumps(out, indent=1) + "\n")
    for ch in ("ledger", "events", "message"):
        r = out[ch]
        print(f"== {ch}: train {r['train']}, test {r['test']}")
        for k in ("jev", "closed_truth", "closed_distill"):
            if k in r:
                v = r[k]
                extra = (f" · escalated {v['escalated']} · {v['latency_ms_mean']} ms/call" if k == "jev" else
                         f" · coverage at Jev's accuracy {v['coverage_at_jev_accuracy']}" +
                         (f" · {v['latency_ms_per_case']} ms/case" if "latency_ms_per_case" in v else "") +
                         f" · T {v['temperature']} · λ {v['lambda']}")
                print(f"  {k:<15} acc {v['accuracy']} {v['accuracy_95']} · ECE(top) {v['ece_top']} {v['ece_top_95']}"
                      f" · log loss {v['log_loss']} · mean top p {v['mean_top_p']}{extra}")
        if "described_open_vocabulary" in r:
            o = r["described_open_vocabulary"]
            print(f"  open vocabulary (described): held-out {o['held_out_accuracy_mean']} · seen {o['seen_accuracy_mean']} · chance {o['chance']}")
            for f in o["folds"]:
                print(f"     {f}")


if __name__ == "__main__":
    main()
