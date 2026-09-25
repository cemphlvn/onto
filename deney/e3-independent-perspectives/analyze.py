#!/usr/bin/env python3
"""E3 analysis, as defined in MEASUREMENT.md. Standard library, fixed seed.

python3 analyze.py ind     # or cor, pilot
"""
import itertools
import json
import math
import random
import sys
from collections import Counter, defaultdict
from pathlib import Path

HERE = Path(__file__).resolve().parent
COLUMNS = ["Message", "Ledger", "Events"]
CHANNEL = {"Message": "message", "Ledger": "ledger", "Events": "events"}
LOOPS = [S for k in (1, 2, 3) for S in itertools.combinations(COLUMNS, k)]
B = 2000


def wilson(k, n, z=1.96):
    if n == 0:
        return [0.0, 1.0]
    p = k / n
    d = 1 + z * z / n
    c = (p + z * z / (2 * n)) / d
    h = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / d
    return [round(max(0.0, c - h), 4), round(min(1.0, c + h), 4)]


def load(cond):
    design = json.loads((HERE / "data/design.json").read_text())
    classes = [c["id"] for c in design["classes"]]
    truth = {k: v for k, v in json.loads((HERE / "data/truth.json").read_text()).items() if v["condition"] == cond}
    walks = {}
    for line in open(HERE / f"runs/{cond}.t.jsonl"):
        e = json.loads(line)
        if e.get("event") == "walk.start" and e.get("case"):
            walks[e["walk"]] = (e["case"], e.get("category"))
    answers = defaultdict(dict)
    for line in open(HERE / f"runs/{cond}.d.jsonl"):
        r = json.loads(line)
        if r["walk"] not in walks or r["at"] != "Read":
            continue
        case, col = walks[r["walk"]]
        o = r["outcome"]
        a = o.get("arrow") if o.get("kind") == "followed" else None
        answers[case][col] = a if a in classes else None
    rows = [{"case": c, **t, "a": {col: answers[c].get(col) for col in COLUMNS}} for c, t in truth.items()]
    return rows, classes, design


def loop_stats(rows, classes, S):
    n = len(rows)
    checked = [r for r in rows if all(r["a"][c] for c in S)]
    wrong = [r for r in checked if any(r["a"][c] != r["truth"] for c in S)]
    undetected = [r for r in wrong if len({r["a"][c] for c in S}) == 1]
    # Independence prediction from per-column answer distributions given truth.
    by_t = defaultdict(list)
    for r in rows:
        by_t[r["truth"]].append(r)
    u_ind = 0.0
    for t, rs in by_t.items():
        for w in classes:
            if w == t:
                continue
            prod = 1.0
            for c in S:
                prod *= sum(1 for r in rs if r["a"][c] == w) / len(rs)
            u_ind += len(rs) / n * prod
    return {"checked": len(checked), "wrong": len(wrong), "undetected": len(undetected),
            "U": len(undetected) / n, "U_ind": u_ind,
            "D": (len(wrong) - len(undetected)) / len(wrong) if wrong else None}


def summarize(rows, classes):
    stats = {S: loop_stats(rows, classes, S) for S in LOOPS}
    e_bar = sum(stats[(c,)]["U"] for c in COLUMNS) / 3
    pts = [(len(S), math.log(stats[S]["U"])) for S in LOOPS if stats[S]["U"] > 0]
    slope = None
    if len({x for x, _ in pts}) > 1:
        mx = sum(x for x, _ in pts) / len(pts)
        my = sum(y for _, y in pts) / len(pts)
        slope = sum((x - mx) * (y - my) for x, y in pts) / sum((x - mx) ** 2 for x, _ in pts)
    return stats, e_bar, slope, len(LOOPS) - len(pts)


def main():
    cond = sys.argv[1]
    rows, classes, design = load(cond)
    n = len(rows)
    out = {"condition": cond, "cases": n}

    # Manipulation check.
    design_n = {c["id"]: c for c in design["conditions"]}
    man = {}
    for col in COLUMNS:
        ch = CHANNEL[col]
        per = {}
        for mode in ("clear", "absent", "misleading"):
            rs = [r for r in rows if r["modes"][ch] == mode]
            if not rs:
                continue
            ok = sum(1 for r in rs if r["a"][col] == (r["truth"] if mode == "clear" else None if mode == "absent"
                                                      else design["neighbour"][r["truth"]]))
            per[mode] = {"n": len(rs), "as_designed": ok, "rate": round(ok / len(rs), 3), "ci": wilson(ok, len(rs))}
        man[col] = per
    out["manipulation"] = man
    out["answers"] = {col: dict(Counter("none" if r["a"][col] is None else
                                        "truth" if r["a"][col] == r["truth"] else
                                        "neighbour" if r["a"][col] == design["neighbour"][r["truth"]] else "other"
                                        for r in rows)) for col in COLUMNS}

    stats, e_bar, slope, dropped = summarize(rows, classes)
    rng = random.Random(0)
    boots = defaultdict(list)
    for _ in range(B):
        sample = [rng.choice(rows) for _ in rows]
        s, e, sl, _ = summarize(sample, classes)
        for S in LOOPS:
            if s[S]["U_ind"] > 0:
                boots[("R", S)].append(s[S]["U"] / s[S]["U_ind"])
            if s[S]["U"] > 0 and 0 < e < 1:
                boots[("k", S)].append(math.log(s[S]["U"]) / math.log(e))
        if sl is not None:
            boots["slope"].append(sl)

    def ci(v):
        v = sorted(v)
        return [round(v[int(0.025 * len(v))], 3), round(v[int(0.975 * len(v)) - 1], 3)] if v else None

    out["loops"] = []
    for S in LOOPS:
        s = stats[S]
        out["loops"].append({
            "loop": "+".join(S), "k": len(S), **{k: s[k] for k in ("checked", "wrong", "undetected")},
            "U": round(s["U"], 4), "U_95": wilson(s["undetected"], n),
            "U_ind": round(s["U_ind"], 4),
            "R": round(s["U"] / s["U_ind"], 3) if s["U_ind"] > 0 else None, "R_95": ci(boots[("R", S)]),
            "D": round(s["D"], 3) if s["D"] is not None else None,
            "D_95": wilson(s["wrong"] - s["undetected"], s["wrong"]) if s["wrong"] else None,
            "k_eff": round(math.log(s["U"]) / math.log(e_bar), 2) if s["U"] > 0 and 0 < e_bar < 1 else None,
            "k_eff_95": ci(boots[("k", S)]),
        })
    out["e_bar"] = round(e_bar, 4)
    out["slope"] = round(slope, 3) if slope is not None else None
    out["slope_95"] = ci(boots["slope"])
    out["slope_if_independent"] = round(math.log(e_bar), 3) if 0 < e_bar < 1 else None
    out["loops_dropped_from_slope_(U=0)"] = dropped

    (HERE / "results").mkdir(exist_ok=True)
    (HERE / f"results/e3.{cond}.json").write_text(json.dumps(out, indent=1) + "\n")
    print(f"condition {cond}: {n} cases")
    print("manipulation check (rate as designed):")
    for col, per in man.items():
        print("  " + col.ljust(8) + "  ".join(f"{m} {v['as_designed']}/{v['n']} ({v['rate']:.2f})" for m, v in per.items()))
    print("answers:", out["answers"])
    print(f"{'loop':<22}{'checked':>8}{'undet':>6}{'U':>8}  {'U 95%':<16}{'U_ind':>8}{'R':>7}  {'R 95%':<16}{'D':>6}{'k_eff':>7}")
    for L in out["loops"]:
        print(f"{L['loop']:<22}{L['checked']:>8}{L['undetected']:>6}{L['U']:>8.4f}  {str(L['U_95']):<16}{L['U_ind']:>8.4f}"
              f"{(L['R'] if L['R'] is not None else float('nan')):>7.2f}  {str(L['R_95']):<16}"
              f"{(L['D'] if L['D'] is not None else float('nan')):>6.2f}{(L['k_eff'] or float('nan')):>7.2f}")
    print(f"ē {out['e_bar']} · slope of ln U on k {out['slope']} {out['slope_95']} · if independent ≈ ln ē = {out['slope_if_independent']}"
          f" · loops dropped (U = 0): {dropped}")


if __name__ == "__main__":
    main()
