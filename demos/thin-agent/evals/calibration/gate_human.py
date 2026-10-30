"""The pipeline end to end, from the records: the gate adopts what the
judge is sure of, a person answers the rest (simulated by the expert
mapping), and the result is scored at class level against that same
mapping. Accuracy against questions asked, per threshold.

    python3 gate_human.py [THRESHOLD ...]

No new judge calls; raw p (the pre-registered identity map).
"""

import json
import sys
from collections import defaultdict
from pathlib import Path

RECORDS = Path(__file__).parent / "records"


def runs():
    for rec in sorted(RECORDS.glob("*.records.jsonl")):
        name = rec.name.removesuffix(".records.jsonl")
        gold = {}
        for line in (RECORDS / f"{name}.gold.jsonl").read_text().splitlines():
            g = json.loads(line)
            gold[g["object"]] = (g["target"], g["classes"])
        records = [json.loads(l) for l in rec.read_text().splitlines()]
        yield name, gold, records


def simulate(gold, records, t):
    """Classes right / total, packages asked, and the judge alone."""
    total = sum(n for _, n in gold.values())
    right = judge_right = asked = 0
    for r in records:
        if r["object"] not in gold:
            continue
        target, n = gold[r["object"]]
        judged = r["target"] if r["disposition"] in ("adopted", "outside") else None
        if r["disposition"] == "adopted" and judged == target:
            judge_right += n
        sure = r["disposition"] == "adopted" and r["p"] is not None and r["p"] >= t
        if sure:
            right += n if judged == target else 0
        else:
            asked += 1
            right += n  # the person answers as the expert did
    return right / total, judge_right / total, asked, len(gold)


def main():
    thresholds = [float(a) for a in sys.argv[1:]] or [0.5, 0.7, 0.9]
    sealed = {"Ant", "Commonsimaging", "Lucene", "Prom", "Sweethome3d"}
    print(f"{'run':40} {'judge alone':>11}  " + "  ".join(f"{'t=' + str(t):>18}" for t in thresholds))
    print(f"{'':40} {'':>11}  " + "  ".join(f"{'accuracy  asked':>18}" for _ in thresholds))
    agg = defaultdict(lambda: [0.0, 0, 0])
    for name, gold, records in runs():
        cells, alone = [], None
        for t in thresholds:
            acc, alone, asked, n = simulate(gold, records, t)
            cells.append(f"{acc:8.3f} {asked:4d}/{n:<4d}")
            if name.split("-")[0] in sealed and "hidden" in name:
                agg[t][0] += acc
                agg[t][1] += asked
                agg[t][2] += n
        print(f"{name:40} {alone:11.3f}  " + "  ".join(cells))
    print("\nsealed systems, names hidden (mean accuracy, packages asked):")
    for t in thresholds:
        acc, asked, n = agg[t]
        print(f"  t={t}: accuracy {acc / 5:.3f}, asked {asked}/{n} packages ({asked / n:.0%})")


if __name__ == "__main__":
    main()
