"""The hierarchical walk of RULE.md: onto's inference engine (frames, the
walk's gate, frame records) routing each package CodeUnit -> group ->
module, on the same input as the flat discover runs.

    python3 walk_eval.py SYSTEM visible|hidden      # SYSTEM: Teammates or Argouml
    python3 walk_eval.py --table

Reads the flat run's category (`../sysmdl/l2-<tag>.onto`) for package and
module descriptions and dependencies, its gold file, and `hierarchy.json`.
"""

import json
import math
import os
import re
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

HERE = Path(__file__).parent
EVALS = HERE.parent
ONTO = os.environ.get("ONTO_BIN", str(HERE.parents[3] / "target/release/onto"))
BP = {"Teammates": "doc", "Argouml": "cookbook"}
GATE = 0.7

sys.path.insert(0, str(EVALS / "baselines"))
from direct_llm import parse  # noqa: E402  (the same reading of the flat run's category)


def quote(s):
    return '"' + s.replace('"', "'") + '"'


def build(system, cond):
    tag = f"{system}-{cond}-bp-{BP[system]}"
    about, deps, modules, mabout = parse(EVALS / "sysmdl" / f"l2-{tag}.onto")
    hierarchy = json.loads((HERE / "hierarchy.json").read_text())[system]
    mid = {re.sub(r"^m_", "", m).replace("_", "."): m for m in modules}  # name in hierarchy -> object id
    # Hierarchy names use the .sysmdl spelling; the flat category's ids replaced dots by underscores.
    resolve = lambda name: next(m for m in modules if m == "m_" + re.sub(r"[^A-Za-z0-9]+", "_", name))
    lines = [f"category Walk{system} {{"]
    objs, arrows, frames, group_of = ["CodeUnit"], [], [], {}
    for g, (gname, members) in enumerate(hierarchy.items()):
        ids = [resolve(m) for m in members]
        if len(ids) == 1 and not gname.startswith("Not placed"):
            arrows.append((f"to_{ids[0]}", "CodeUnit", ids[0], mabout[ids[0]]))
            group_of[ids[0]] = gname
            continue
        if gname.startswith("Not placed"):
            for i in ids:
                arrows.append((f"to_{i}", "CodeUnit", i, mabout[i]))
                group_of[i] = gname
            continue
        gid = f"g{g}"
        objs.append(gid)
        text = gname + ". Its modules: " + "; ".join(mabout[i] for i in ids)
        arrows.append((f"to_{gid}", "CodeUnit", gid, text))
        frames.append((gid, f"Which module of this part ({gname.split(':')[0]}) is responsible for the code unit?"))
        for i in ids:
            arrows.append((f"is_{i}", gid, i, mabout[i]))
            group_of[i] = gname
    objs += modules
    lines.append(f"    objects: {', '.join(objs)};")
    lines.append("    start: CodeUnit;")
    lines.append("    admission: sealed;")
    lines.append('    frame CodeUnit: choice "Which part of the intended architecture is responsible for this code unit?";')
    for gid, q in frames:
        lines.append(f"    frame {gid}: choice {quote(q)};")
    for name, a, b, text in arrows:
        lines.append(f"    {name}: {a} -> {b} {quote(text)};")
    lines.append(f"    closed: {', '.join(objs)};")
    lines.append("}")
    path = HERE / f"walk-{tag}.onto"
    path.write_text("\n".join(lines) + "\n")
    order = sorted(about)
    jobs = HERE / f"walk-{tag}.jobs"
    jobs.write_text("".join(
        "CodeUnit: " + re.sub(r"\s+", " ", f"Code unit: {about[o]}. It depends on: "
                              + (", ".join(about.get(d, d) for d in deps.get(o, [])[:20]) or "nothing")) + "\n"
        for o in order))
    return tag, path, jobs, order, group_of


def run(system, cond):
    tag, path, jobs, order, group_of = build(system, cond)
    report = HERE / f"walk-{tag}.report.json"
    r = subprocess.run([ONTO, "run", str(path), "--jobs", str(jobs), "--mock-proposer", "--closed-world",
                        "--threshold", "0", "--report", str(report), "--dispositions", str(HERE / f"walk-{tag}.frames.jsonl")],
                       capture_output=True, text=True)
    print(r.stdout[-600:] + r.stderr[-600:], flush=True)
    if r.returncode:
        sys.exit(r.returncode)
    score(system, cond)


def score(system, cond):
    tag, path, jobs, order, group_of = build(system, cond)
    rep = json.loads((HERE / f"walk-{tag}.report.json").read_text())
    walks = sorted((w for w in rep["walks"] if w.get("parent") is None), key=lambda w: w["walk"])
    assert len(walks) == len(order), (len(walks), len(order))
    gold = {}
    for line in (EVALS / "calibration" / "records" / f"{tag}.gold.jsonl").read_text().splitlines():
        g = json.loads(line)
        gold[g["object"]] = (g["target"], g["classes"])
    total = sum(n for _, n in gold.values())
    right = wrong_group = wrong_module = escalated = 0
    gated_right = gated_classes = 0
    frames = []
    for obj, w in zip(order, walks):
        if obj not in gold:
            continue
        target, n = gold[obj]
        end = w["path"].split("->")[-1].strip()
        ps = []
        for f in w["frames"]:
            sel = [c for c in f["candidates"] if c["disposition"]["kind"] == "selected"]
            if sel:
                ps.append(sel[0]["judgment"])
                frames.append((sel[0]["judgment"], None))
        path_p = math.prod(ps) if ps else 0.0
        ok = end == target
        if not end.startswith("m_"):
            escalated += n
        elif ok:
            right += n
        elif group_of.get(end) != group_of.get(target):
            wrong_group += n
        else:
            wrong_module += n
        if end.startswith("m_") and path_p >= GATE:
            gated_classes += n
            gated_right += n if ok else 0
    flat = json.loads((EVALS / "sysmdl" / f"l2-{tag}.json").read_text())["discover"]
    flat_gate = flat_gate_right = 0
    for line in (EVALS / "calibration" / "records" / f"{tag}.records.jsonl").read_text().splitlines():
        r = json.loads(line)
        if r["object"] in gold and r["disposition"] == "adopted" and (r["p"] or 0) >= GATE:
            t, n = gold[r["object"]]
            flat_gate += n
            flat_gate_right += n if r["target"] == t else 0
    out = dict(system=system, names=cond, classes=total, walk_accuracy=right / total, flat_accuracy=flat["accuracy"],
               wrong_at_group=wrong_group / total, wrong_at_module=wrong_module / total, escalated=escalated / total,
               walk_gate_coverage=gated_classes / total, walk_gate_accuracy=gated_right / gated_classes if gated_classes else None,
               flat_gate_coverage=flat_gate / total, flat_gate_accuracy=flat_gate_right / flat_gate if flat_gate else None,
               judge_calls=rep.get("judge_calls"), tokens_in=rep.get("tokens_in"))
    (HERE / f"walk-{tag}.result.json").write_text(json.dumps(out, indent=1) + "\n")
    print(json.dumps(out, indent=1))


def table():
    rows = [json.loads(f.read_text()) for f in sorted(HERE.glob("walk-*.result.json"))]
    print(f"{'system':10} {'names':8} {'walk':>6} {'flat':>6} {'Δ':>6} | wrong@group wrong@module escalated | gate p≥{GATE}: walk cov/acc   flat cov/acc")
    for r in rows:
        print(f"{r['system']:10} {r['names']:8} {r['walk_accuracy']:6.3f} {r['flat_accuracy']:6.3f} {r['walk_accuracy'] - r['flat_accuracy']:+6.3f} | "
              f"{r['wrong_at_group']:11.3f} {r['wrong_at_module']:12.3f} {r['escalated']:9.3f} | "
              f"{r['walk_gate_coverage']:.2f}/{(r['walk_gate_accuracy'] or 0):.2f}   {r['flat_gate_coverage']:.2f}/{(r['flat_gate_accuracy'] or 0):.2f}")


if __name__ == "__main__":
    if sys.argv[1] == "--table":
        table()
    else:
        run(sys.argv[1], sys.argv[2])
