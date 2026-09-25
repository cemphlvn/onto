#!/usr/bin/env python3
"""E2 analysis: boundary cases, calibration, and the team loop as a label.

Per case: the intent column's judgment (40 intents), the team column's
(7 teams), and the ensemble's verdict (agreed: the loop resonates;
surprise: it does not; incomplete: a column escalated). Labels are the
generation targets (synthetic); `even` cases have none. Standard library,
fixed seed. From this folder:  python3 analyze.py
"""
import json
import random
import re
import statistics as st
from collections import Counter, defaultdict
from pathlib import Path

HERE = Path(__file__).resolve().parent
BINS = 10
LEVELS = ["clear_a", "lean_a", "even", "lean_b", "clear_b"]
random.seed(0)


def ece(pairs):
    bins = defaultdict(list)
    for p, y in pairs:
        bins[min(int(p * BINS), BINS - 1)].append((p, y))
    return sum(len(b) / len(pairs) * abs(st.mean(p for p, _ in b) - st.mean(y for _, y in b))
               for b in bins.values()) if pairs else None


def boot(by_case, stat, n=1000):
    cases = list(by_case)
    vals = sorted(v for v in (stat([x for c in random.choices(cases, k=len(cases)) for x in by_case[c]])
                              for _ in range(n)) if v is not None)
    return [round(vals[int(0.025 * len(vals))], 3), round(vals[int(0.975 * len(vals)) - 1], 3)]


def main():
    labels = json.loads((HERE / "data/labels.json").read_text())
    src = (HERE / "data/boundary.onto").read_text()
    functor = src[src.index("functor ByTeam"):]
    team_of = {a: t.capitalize() for a, t in re.findall(r"([a-z_]+): ([a-z]+);", functor)}

    walks = {}
    for line in open(HERE / "runs/teamloop.t.jsonl"):
        e = json.loads(line)
        if e.get("event") == "walk.start" and e.get("case"):
            walks[e["walk"]] = (e["case"], e.get("category"))
    judged = defaultdict(dict)  # case -> column -> record
    for line in open(HERE / "runs/teamloop.d.jsonl"):
        r = json.loads(line)
        if r.get("judge") and r["walk"] in walks:
            case, cat = walks[r["walk"]]
            judged[case][cat] = r
    report = json.loads((HERE / "runs/teamloop.report.json").read_text())
    status = {c["case"]: c["status"] for c in report["cases"]}

    def taken(r):
        return r["outcome"].get("arrow") if r and r["outcome"].get("kind") == "followed" else None

    rows = []
    for cid, lab in labels.items():
        ir, tr = judged[cid].get("SupportLarge"), judged[cid].get("Teams")
        ps = {c["arrow"]: c["judgment"] for c in (ir or {}).get("candidates", []) if c.get("judgment") is not None}
        rows.append({
            "case": cid, **lab, "status": status.get(cid),
            "intent_taken": taken(ir), "team_taken": taken(tr),
            "intent_conf": (ir or {}).get("judge", {}).get("confidence"),
            "team_conf": (tr or {}).get("judge", {}).get("confidence"),
            "p": ps, "team_ps": {c["arrow"]: c["judgment"] for c in (tr or {}).get("candidates", [])
                                 if c.get("judgment") is not None},
        })

    out = {}
    # H1: how much mass in the middle.
    intent_sw = [p for r in rows for p in r["p"].values()]
    team_sw = [p for r in rows for p in r["team_ps"].values()]
    tops = [max(r["p"].values()) for r in rows if r["p"]] + [max(r["team_ps"].values()) for r in rows if r["team_ps"]]
    out["H1"] = {
        "intent_switches": len(intent_sw), "intent_mid": sum(0.1 <= p < 0.9 for p in intent_sw),
        "team_switches": len(team_sw), "team_mid": sum(0.1 <= p < 0.9 for p in team_sw),
        "calls": len(tops), "calls_top_below_0.9": sum(t < 0.9 for t in tops),
        "mean_top_p": round(st.mean(tops), 3),
        "E1_reference": {"mid": "21 of 2889 switches", "calls_top_below_0.9": "12 of 147", "mean_top_p": 0.973},
    }
    # Dose-response: p(A) share within the pair, by level.
    dose = {}
    for lv in LEVELS:
        vals = [r["p"].get(r["a"], 0) / (r["p"].get(r["a"], 0) + r["p"].get(r["b"], 0))
                for r in rows if r["level"] == lv and (r["p"].get(r["a"], 0) + r["p"].get(r["b"], 0)) > 0]
        dose[lv] = {"n": len(vals), "mean_share_a": round(st.mean(vals), 3) if vals else None}
    out["dose_response"] = dose
    # Where decisions went.
    lab_rows = [r for r in rows if r["intent"]]
    outside = [r for r in lab_rows if r["intent_taken"] and r["intent_taken"] not in (r["a"], r["b"])]
    out["decisions"] = {
        "labelled": len(lab_rows),
        "intent_right": sum(r["intent_taken"] == r["intent"] for r in lab_rows),
        "intent_other_side": sum(r["intent_taken"] in (r["a"], r["b"]) and r["intent_taken"] != r["intent"] for r in lab_rows),
        "intent_outside_pair": len(outside),
        "intent_escalated": sum(r["intent_taken"] is None for r in lab_rows),
        "outside_examples": [{"case": r["case"], "label": r["intent"], "taken": r["intent_taken"]} for r in outside[:12]],
        "by_level": {lv: dict(Counter("right" if r["intent_taken"] == r["intent"] else
                                      "escalated" if r["intent_taken"] is None else
                                      "outside" if r["intent_taken"] not in (r["a"], r["b"]) else "other_side"
                                      for r in lab_rows if r["level"] == lv)) for lv in LEVELS if lv != "even"},
    }
    # H3: calibration of the intent switches on labelled cases.
    by_case = defaultdict(list)
    for r in lab_rows:
        for arrow, p in r["p"].items():
            by_case[r["case"]].append((p, 1 if arrow == r["intent"] else 0))
    pairs = [x for v in by_case.values() for x in v]
    rel = defaultdict(list)
    for p, y in pairs:
        rel[min(int(p * BINS), BINS - 1)].append((p, y))
    out["H3"] = {
        "switches": len(pairs), "ece": round(ece(pairs), 4), "ece_95": boot(by_case, ece),
        "brier": round(st.mean((p - y) ** 2 for p, y in pairs), 4),
        "reliability": [{"bin": f"{k / BINS:.1f}-{(k + 1) / BINS:.1f}", "n": len(b),
                         "mean_p": round(st.mean(p for p, _ in b), 3),
                         "frequency": round(st.mean(y for _, y in b), 3)} for k, b in sorted(rel.items())],
        "decision_confidence_vs_right": {
            f"{lo:.1f}-{lo + 0.1:.1f}": {"n": len(g), "right": sum(g)}
            for lo in [x / 10 for x in range(10)]
            for g in [[r["intent_taken"] == r["intent"] for r in lab_rows
                       if r["intent_taken"] and r["intent_conf"] is not None and lo <= r["intent_conf"] < lo + 0.1]] if g},
    }
    # H2: the loop as a label, when both columns followed.
    h2 = {}
    for kind in ("within", "cross"):
        both = [r for r in lab_rows if r["kind"] == kind and r["intent_taken"] and r["team_taken"]]
        table = Counter((r["status"], "intent_right" if r["intent_taken"] == r["intent"] else "intent_wrong") for r in both)
        wrong = [r for r in both if r["intent_taken"] != r["intent"]]
        right = [r for r in both if r["intent_taken"] == r["intent"]]
        h2[kind] = {
            "cases_both_followed": len(both),
            "table": {f"{s} / {k}": n for (s, k), n in sorted(table.items())},
            "surprise_when_intent_wrong": f"{sum(r['status'] == 'surprise' for r in wrong)}/{len(wrong)}",
            "surprise_when_intent_right": f"{sum(r['status'] == 'surprise' for r in right)}/{len(right)}",
            "wrong_intent_same_team": sum(team_of.get(r["intent_taken"]) == team_of.get(r["intent"]) for r in wrong),
            "team_right": sum(r["team_taken"] and r["team_taken"].capitalize() == team_of.get(r["intent"]) for r in both),
        }
    even = [r for r in rows if r["level"] == "even"]
    h2["even_cases"] = dict(Counter(r["status"] for r in even))
    h2["all_status"] = dict(Counter(r["status"] for r in rows))
    h2["incomplete_by_column"] = dict(Counter(
        ("intent escalated" if r["intent_taken"] is None else "") + (" team escalated" if r["team_taken"] is None else "")
        for r in rows if r["status"] == "incomplete"))
    out["H2"] = h2

    # Even cases: does Jev split them, or pick a side?
    shares = [r["p"].get(r["a"], 0) / (r["p"].get(r["a"], 0) + r["p"].get(r["b"], 0))
              for r in even if (r["p"].get(r["a"], 0) + r["p"].get(r["b"], 0)) > 0]
    out["even_split"] = {"n": len(shares),
                         "decisive (share < 0.1 or > 0.9)": sum(x < 0.1 or x > 0.9 for x in shares),
                         "split (0.3 to 0.7)": sum(0.3 <= x <= 0.7 for x in shares)}
    # Top-label calibration: the probability of the arrow taken vs whether it was right.
    top = defaultdict(list)
    for r in lab_rows:
        if r["intent_taken"]:
            p = r["p"][r["intent_taken"]]
            top[min(int(p * BINS), BINS - 1)].append((p, 1 if r["intent_taken"] == r["intent"] else 0))
    allt = [x for v in top.values() for x in v]
    out["top_label"] = {"decisions": len(allt), "mean_p": round(st.mean(p for p, _ in allt), 3),
                        "accuracy": round(st.mean(y for _, y in allt), 3), "ece": round(ece(allt), 4),
                        "bins": [{"bin": f"{k / BINS:.1f}-{(k + 1) / BINS:.1f}", "n": len(b),
                                  "mean_p": round(st.mean(p for p, _ in b), 3),
                                  "accuracy": round(st.mean(y for _, y in b), 3)} for k, b in sorted(top.items())]}
    out["confident_errors"] = [{"case": r["case"], "label": r["intent"], "taken": r["intent_taken"],
                                "p": round(r["p"][r["intent_taken"]], 3), "team_taken": r["team_taken"],
                                "status": r["status"]}
                               for r in lab_rows if r["intent_taken"] and r["intent_taken"] != r["intent"]]
    out["loop_surprises"] = [{"case": r["case"], "label": r["intent"], "intent_taken": r["intent_taken"],
                              "team_taken": r["team_taken"], "team_expected": team_of.get(r["intent"]) if r["intent"] else None}
                             for r in rows if r["status"] == "surprise"]

    (HERE / "results").mkdir(exist_ok=True)
    (HERE / "results/e2.json").write_text(json.dumps(out, indent=1, default=str) + "\n")
    print(json.dumps(out, indent=1, default=str))


if __name__ == "__main__":
    main()
