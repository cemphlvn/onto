#!/usr/bin/env python3
"""E1: Jev's calibration per switch, against labels written before the runs.

A switch is one judged candidate (one-vs-rest): its probability p is the
judgment the record keeps, its label y is 1 when the candidate is the
expected arrow of that labelled frame. A decision is the frame's outcome:
followed (right or wrong arrow) or escalated, with the confidence Jev
reported. Standard library only; fixed seed. From this folder:

    python3 analyze.py
"""
import json
import math
import random
from collections import defaultdict
from pathlib import Path

HERE = Path(__file__).resolve().parent
SETS = ["support-large", "incident-perspectives", "discharge-perspectives"]
BINS = 10
random.seed(0)


def load(set_name, rep):
    walks = {}
    for line in open(HERE / f"runs/{set_name}.{rep}.t.jsonl"):
        e = json.loads(line)
        if e.get("event") == "walk.start" and e.get("case"):
            walks[e["walk"]] = (e["case"], e.get("category"))
    records = [json.loads(l) for l in open(HERE / f"runs/{set_name}.{rep}.d.jsonl") if l.strip()]
    return walks, records


def ece(pairs):
    """Expected calibration error, equal-width bins, weighted by bin size."""
    if not pairs:
        return None
    bins = defaultdict(list)
    for p, y in pairs:
        bins[min(int(p * BINS), BINS - 1)].append((p, y))
    total = len(pairs)
    return sum(len(b) / total * abs(sum(p for p, _ in b) / len(b) - sum(y for _, y in b) / len(b))
               for b in bins.values())


def reliability(pairs):
    bins = defaultdict(list)
    for p, y in pairs:
        bins[min(int(p * BINS), BINS - 1)].append((p, y))
    return [{"bin": f"{k / BINS:.1f}-{(k + 1) / BINS:.1f}", "n": len(b),
             "mean_p": round(sum(p for p, _ in b) / len(b), 3),
             "frequency": round(sum(y for _, y in b) / len(b), 3)}
            for k, b in sorted(bins.items())]


def brier(pairs):
    return sum((p - y) ** 2 for p, y in pairs) / len(pairs) if pairs else None


def logloss(pairs):
    eps = 1e-4
    return -sum(y * math.log(max(p, eps)) + (1 - y) * math.log(max(1 - p, eps))
                for p, y in pairs) / len(pairs) if pairs else None


def boot(by_case, stat, n=1000):
    """95% interval of a statistic, resampling cases (switches of one case stay together)."""
    cases = list(by_case)
    vals = []
    for _ in range(n):
        sample = [pair for c in random.choices(cases, k=len(cases)) for pair in by_case[c]]
        v = stat(sample)
        if v is not None:
            vals.append(v)
    vals.sort()
    return [round(vals[int(0.025 * len(vals))], 3), round(vals[int(0.975 * len(vals)) - 1], 3)]


def main():
    labels = json.load(open(HERE / "data/labels.json"))["sets"]
    out = {"sets": {}, "all": {}}
    all_switches, all_by_case, all_decisions = [], defaultdict(list), []
    for s in SETS:
        cases = labels[s]["cases"]
        switches, by_case, decisions = [], defaultdict(list), []
        per_rep_p = defaultdict(list)       # (case, frame, arrow) -> p per repeat
        per_rep_choice = defaultdict(list)  # (case, frame) -> decision per repeat
        reached = 0
        for rep in (1, 2, 3):
            walks, records = load(s, rep)
            for r in records:
                if not r.get("judge") or r["walk"] not in walks:
                    continue
                case, category = walks[r["walk"]]
                frame = f"{category}:{r['at']}"
                expected = cases.get(case, {}).get(frame)
                if expected is None:
                    continue
                reached += 1
                for c in r["candidates"]:
                    if c.get("judgment") is None:
                        continue
                    pair = (c["judgment"], 1 if c["arrow"] == expected else 0)
                    switches.append(pair)
                    by_case[f"{s}/{case}"].append(pair)
                    per_rep_p[(case, frame, c["arrow"])].append(c["judgment"])
                o = r["outcome"]
                taken = o.get("arrow") if o.get("kind") == "followed" else None
                decisions.append({"case": case, "frame": frame, "expected": expected, "taken": taken,
                                  "confidence": r["judge"].get("confidence"),
                                  "none_of_these": r["judge"].get("none_of_these")})
                per_rep_choice[(case, frame)].append(taken)
        labelled = sum(len(v) for v in cases.values()) * 3
        followed = [d for d in decisions if d["taken"]]
        right = [d for d in followed if d["taken"] == d["expected"]]
        conf_pairs = [(d["confidence"], 1 if d["taken"] == d["expected"] else 0)
                      for d in followed if d["confidence"] is not None]
        spread = [max(v) - min(v) for v in per_rep_p.values() if len(v) == 3]
        stable = [len(set(v)) == 1 for v in per_rep_choice.values() if len(v) == 3]
        out["sets"][s] = {
            "labelled_frames": labelled, "reached": reached,
            "switches": len(switches), "positives": sum(y for _, y in switches),
            "ece": round(ece(switches), 4), "ece_95": boot(by_case, ece),
            "brier": round(brier(switches), 4), "log_loss": round(logloss(switches), 4),
            "decisions": len(decisions), "followed": len(followed), "right": len(right),
            "escalated": len(decisions) - len(followed),
            "wrong": [{k: d[k] for k in ("case", "frame", "expected", "taken", "confidence")}
                      for d in followed if d["taken"] != d["expected"]],
            "escalations": [{k: d[k] for k in ("case", "frame", "expected", "confidence", "none_of_these")}
                            for d in decisions if not d["taken"]],
            "confidence_ece": round(ece(conf_pairs), 4) if conf_pairs else None,
            "repeat_p_spread_max": round(max(spread), 4) if spread else None,
            "repeat_p_spread_mean": round(sum(spread) / len(spread), 5) if spread else None,
            "same_decision_all_repeats": f"{sum(stable)}/{len(stable)}",
            "reliability": reliability(switches),
        }
        all_switches += switches
        all_decisions += decisions
        for k, v in by_case.items():
            all_by_case[k] += v
    followed = [d for d in all_decisions if d["taken"]]
    out["all"] = {
        "switches": len(all_switches), "positives": sum(y for _, y in all_switches),
        "ece": round(ece(all_switches), 4), "ece_95": boot(all_by_case, ece),
        "brier": round(brier(all_switches), 4), "log_loss": round(logloss(all_switches), 4),
        "decisions": len(all_decisions), "followed": len(followed),
        "right": sum(1 for d in followed if d["taken"] == d["expected"]),
        "reliability": reliability(all_switches),
    }
    (HERE / "results").mkdir(exist_ok=True)
    (HERE / "results/e1.json").write_text(json.dumps(out, indent=1) + "\n")

    for s, r in out["sets"].items():
        print(f"{s}: {r['switches']} switches ({r['positives']} positive) · ECE {r['ece']} {r['ece_95']} · "
              f"Brier {r['brier']} · log loss {r['log_loss']}")
        print(f"   decisions {r['decisions']}/{r['labelled_frames']} labelled frames reached · followed {r['followed']} "
              f"(right {r['right']}) · escalated {r['escalated']} · confidence ECE {r['confidence_ece']} · "
              f"repeats: same decision {r['same_decision_all_repeats']}, p spread max {r['repeat_p_spread_max']}")
        for w in r["wrong"]:
            print(f"   WRONG {w}")
        for e in r["escalations"]:
            print(f"   escalated {e}")
    a = out["all"]
    print(f"ALL: {a['switches']} switches · ECE {a['ece']} {a['ece_95']} · Brier {a['brier']} · "
          f"decisions right {a['right']}/{a['followed']} followed of {a['decisions']}")
    for b in a["reliability"]:
        print(f"   p {b['bin']}: n {b['n']:>4} · mean p {b['mean_p']:.3f} · frequency {b['frequency']:.3f}")


if __name__ == "__main__":
    main()
