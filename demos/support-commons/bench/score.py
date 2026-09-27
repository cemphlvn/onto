"""Score frame records (`onto run --dispositions`) against labels.json.

usage: python3 score.py <dispositions.jsonl> [wall seconds]
"""

import json
import sys
from pathlib import Path

labels = json.loads((Path(__file__).parent / "labels.json").read_text())
rows = {}
for line in open(sys.argv[1]):
    r = json.loads(line)
    if r["at"] != "Ticket" or r["id"] in rows:
        continue
    case = r["seen"]["case"]["id"]
    if case in {x["case"] for x in rows.values()}:
        continue  # the first visit of the frame per ticket
    ps = {c["arrow"]: c["judgment"] or 0.0 for c in r["candidates"]}
    top = max(ps, key=ps.get)
    sel = next((c["arrow"] for c in r["candidates"] if c["disposition"]["kind"] == "selected"), None)
    rows[r["id"]] = {"case": case, "top": top, "p": ps[top], "p_label": ps.get(labels[case], 0.0),
                     "selected": sel, "ms": r["judge"]["latency_ms"], "model": r["judge"]["model"]}

n = len(rows)
top1 = sum(x["top"] == labels[x["case"]] for x in rows.values())
answered = [x for x in rows.values() if x["selected"]]
right = sum(x["selected"] == labels[x["case"]] for x in answered)
out = {
    "model": next(iter(rows.values()))["model"] if rows else None,
    "tickets": n,
    "top1": top1,
    "engine_correct": right,
    "answered": len(answered),
    "precision": round(right / len(answered), 2) if answered else None,
    "judge_ms_mean": round(sum(x["ms"] for x in rows.values()) / max(n, 1), 1),
    "wall_s": float(sys.argv[2]) if len(sys.argv) > 2 else None,
    "per_ticket": {x["case"]: {"label": labels[x["case"]], "top": x["top"], "p": round(x["p"], 3),
                               "p_label": round(x["p_label"], 3), "selected": x["selected"]}
                   for x in sorted(rows.values(), key=lambda x: int(x["case"].split("-")[1]))},
}
print(json.dumps(out, ensure_ascii=False, indent=1))
