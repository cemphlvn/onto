#!/usr/bin/env python3
"""E5b: the self-building loop with a novelty signal (MEASUREMENT.md).

Primary stream: E4's 540 messages + 120 from 2 new situations, all judged
by Jev on data/message10.onto (runs/stream.*). Secondary: E5's stream.
Learner code from ../e4-cpu-switch (same lineage). No model calls.

    python3 simulate.py        # results/e5b.json
"""
import json
import sys
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE / "../e4-cpu-switch"))
import learn as E  # noqa: E402

ORDERS = range(10)
RETRAIN, CPU_GATE, JEV_GATE = 30, 0.8, 0.6
MIN_LABELS, AUDIT, AUDIT_WINDOW, AUDIT_AGREE, NEW_MIN = 90, 0.10, 20, 0.9, 10
POLICIES = ["L0", "L1", "L2", "L3", "L4"]


# ------------------------------------------------------------- streams
def primary():
    labels = [c for c in json.loads((HERE / "../e3-independent-perspectives/data/design.json").read_text())["classes"]]
    LAB = [c["id"] for c in labels] + ["subscription_cancel", "address_change", "none"]
    idx = {k: i for i, k in enumerate(LAB)}
    truth = json.loads((HERE / "data/truth.json").read_text())
    texts = {}
    for line in open(HERE / "data/stream.jobs"):
        c = json.loads(line[6:])
        texts[c["id"]] = c["message"]
    walks = {}
    for line in open(HERE / "runs/stream.t.jsonl"):
        e = json.loads(line)
        if e.get("event") == "walk.start" and e.get("case"):
            walks[e["walk"]] = e["case"]
    cases = []
    for line in open(HERE / "runs/stream.d.jsonl"):
        r = json.loads(line)
        if r["walk"] not in walks or r["at"] != "Read" or not r.get("judge"):
            continue
        cid = walks[r["walk"]]
        p = np.zeros(len(LAB))
        for c in r["candidates"]:
            if c.get("judgment") is not None:
                p[idx.get(c["arrow"], idx["none"]) if c["arrow"] != "no_signal" else idx["none"]] += c["judgment"]
        p[idx["none"]] += r["judge"].get("none_of_these") or 0.0
        p = p / p.sum()
        t = truth[cid]
        cases.append({"text": texts[cid], "y": idx[t["truth"] or "none"], "late": t["new"], "p": p})
    return cases, LAB, 330


def secondary():
    sys.path.insert(0, str(HERE / "../e5-self-building-loop"))
    truth = E.load_truth()
    cases = []
    for split in ("train", "test"):
        jobs = E.read_jobs(f"{split}.message")
        J = E.jev(f"{split}.message")
        for cid, c in jobs.items():
            t = truth[cid]["truth"] or "none"
            cases.append({"text": E.text_of(c, "message"), "y": E.L[t], "late": t in ("wrong_item", "account_takeover"), "p": J[cid][0]})
    return cases, E.LABELS, 270


def order(cases, first_n, seed):
    rng = np.random.default_rng(seed)
    early = [c for c in cases if not c["late"]]
    late = [c for c in cases if c["late"]]
    rng.shuffle(early)
    first, rest = early[:first_n], early[first_n:] + late
    rng.shuffle(rest)
    return first + rest


# ------------------------------------------------------------- learner
def train(texts, Y):
    X = E.featurize(texts)
    cut = int(len(texts) * 0.75)
    W, b = E.fit_softmax(X[:cut], Y[:cut], 1e-4)
    T = E.fit_temperature(X[cut:] @ W + b, Y[cut:]) if len(texts) - cut >= 5 else 1.0
    S = (X @ X.T).toarray()
    np.fill_diagonal(S, -1)
    tau = float(np.percentile(S.max(1), 5))
    return {"W": W, "b": b, "T": T, "X": X, "tau": tau}


def run(cases, labels, first_n, seed, policy):
    s = order(cases, first_n, seed)
    k = len(labels)
    eye = np.eye(k)
    texts, targets, model = [], [], None
    known, audits, suspended = set(), [], set()
    rng = np.random.default_rng(1000 + seed)
    log, novelty = [], []
    for i, c in enumerate(s):
        if i and i % RETRAIN == 0 and len(texts) >= 20:
            model = train(texts, np.array(targets))
            counts = np.bincount([int(np.argmax(t)) for t in targets], minlength=k)
            known = {j for j in range(k) if counts[j] > 0}
            suspended = {j for j in suspended if counts[j] < NEW_MIN}
        by = None
        if model is not None:
            x = E.featurize([c["text"]])
            P = E.softmax((x @ model["W"] + model["b"]) / model["T"])[0]
            sim = float((model["X"] @ x.T).toarray().max())
            is_novel = sim < model["tau"]
            novelty.append({"score": 1 - sim, "new_to_learner": c["y"] not in known})
            ok = P.max() >= CPU_GATE
            if policy in ("L1", "L3", "L4"):
                ok = ok and len(texts) >= MIN_LABELS
            if policy in ("L2", "L3"):
                ok = ok and not is_novel
            if policy == "L4":
                recent = audits[-AUDIT_WINDOW:]
                ok = ok and not suspended and (len(recent) < 5 or np.mean(recent) >= AUDIT_AGREE)
                if ok and rng.random() < AUDIT:
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
        base_by = "jev" if c["p"].max() >= JEV_GATE else "person"
        base_ans = int(c["p"].argmax()) if base_by == "jev" else c["y"]
        log.append({"late": c["late"], "by": by, "right": ans == c["y"], "base_right": base_ans == c["y"], "base_by": base_by})
    return log, novelty


def auroc(pos, neg):
    """P(score of a new case > score of a known case), ties counted half (Mann-Whitney)."""
    if not pos or not neg:
        return None
    from scipy.stats import rankdata
    r = rankdata(list(pos) + list(neg))
    return float((r[:len(pos)].sum() - len(pos) * (len(pos) + 1) / 2) / (len(pos) * len(neg)))


def summarize(runs):
    per = []
    for log, _ in runs:
        n = len(log)
        calls = sum(r["by"] == "jev" for r in log) + 0
        per.append({"per_call": n / max(calls, 1), "accuracy": float(np.mean([r["right"] for r in log])),
                    "base_accuracy": float(np.mean([r["base_right"] for r in log])),
                    "person": sum(r["by"] == "person" for r in log), "base_person": sum(r["base_by"] == "person" for r in log),
                    "cpu_share": float(np.mean([r["by"] == "cpu" for r in log])),
                    "cpu_errors_known": sum(r["by"] == "cpu" and not r["right"] and not r["late"] for r in log),
                    "cpu_errors_new": sum(r["by"] == "cpu" and not r["right"] and r["late"] for r in log)})
    agg = {}
    for k in per[0]:
        v = [p[k] for p in per]
        agg[k] = {"mean": round(float(np.mean(v)), 3), "min": round(float(np.min(v)), 3), "max": round(float(np.max(v)), 3)}
    return agg


def main():
    out = {}
    for name, (cases, labels, first_n) in {"primary": primary(), "secondary": secondary()}.items():
        res = {}
        for pol in POLICIES:
            runs = [run(cases, labels, first_n, s, pol) for s in ORDERS]
            res[pol] = summarize(runs)
            if pol == "L0":
                nov = [x for _, nv in runs for x in nv]
                res["detector_auroc"] = round(auroc([x["score"] for x in nov if x["new_to_learner"]],
                                                    [x["score"] for x in nov if not x["new_to_learner"]]) or float("nan"), 3)
                res["detector_cases"] = {"new_to_learner": sum(x["new_to_learner"] for x in nov),
                                         "known": sum(not x["new_to_learner"] for x in nov)}
            print(name, pol, {k: res[pol][k]["mean"] for k in res[pol]}, flush=True)
        # Jev calls include audits for L4? Audited cases were answered by Jev: counted as "jev".
        out[name] = res
        print(name, "detector AUROC", res["detector_auroc"], res["detector_cases"], flush=True)
    (HERE / "results").mkdir(exist_ok=True)
    (HERE / "results/e5b.json").write_text(json.dumps(out, indent=1) + "\n")


if __name__ == "__main__":
    main()
