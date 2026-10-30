"""The all-judges leaderboard on the architecture-mapping task: every judge
runs behind onto's Judge contract on the same frames (8 systems, names
visible and hidden, documentation blueprints), and is scored from its
decision records.

    python3 leaderboard.py run JUDGE [--jobs=N]   # JUDGE as onto discover --judge-model; "jev" for the default
    python3 leaderboard.py status [JUDGE ...]     # runs done / total per judge, and runs in flight
    python3 leaderboard.py table [--price JUDGE=IN,OUT ...]   # USD per million tokens

`sources.local.json` (not committed) maps each system to its source
directories on this machine. Scoring is package level (one decision,
one point; JabRef's class-level results are in EVALS.md), so every judge
is compared on the same 361 decisions per condition set.
"""

import json
import math
import random
import re
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

HERE = Path(__file__).parent
RECORDS = HERE / "records"
EVALS = HERE.parent
SYSTEMS = [  # name, script, sysmdl, blueprint, conditions
    ("Teammates", "sysmdl", "data/teammates.sysmdl", "teammates.doc.json", ("visible", "hidden")),
    ("Argouml", "sysmdl", "data/argouml.sysmdl", "argouml.cookbook.json", ("visible", "hidden")),
    ("Prom", "sysmdl", "data/ProM_6_9.sysmdl", "prom.doc.json", ("visible", "hidden")),
    ("Lucene", "sysmdl", "data/lucene.sysmdl", "lucene.doc.json", ("visible", "hidden")),
    ("Commonsimaging", "sysmdl", "data/commons-imaging.sysmdl", "commons-imaging.doc.json", ("visible", "hidden")),
    ("Ant", "sysmdl", "data/ant.sysmdl", "ant.doc.json", ("visible", "hidden")),
    ("Sweethome3d", "sysmdl", "data/sweethome3d.sysmdl", "sweethome3d.doc.json", ("visible", "hidden")),
    ("JabRef", "jabref", None, None, ("visible",)),
]
SEALED = {"Prom", "Lucene", "Commonsimaging", "Ant", "Sweethome3d"}


def slug(judge):
    return re.sub(r"[^A-Za-z0-9.]+", "_", judge)


def record_file(system, cond, judge):
    suffix = "" if judge == "jev" else f"-j-{slug(judge)}"
    if system == "JabRef":
        tag = "JabRef-meaning-noroles-framing-arch" + ("" if judge == "jev" else f"-judge-{slug(judge)}")
    else:
        bp = dict((s[0], s[3]) for s in SYSTEMS)[system].split(".")[1]
        tag = f"{system}-{cond}-bp-{bp}{suffix}"
    return RECORDS / f"{tag}.records.jsonl", RECORDS / f"{tag}.gold.jsonl"


def tasks():
    return [(system, cond) for system, _, _, _, conds in SYSTEMS for cond in conds]


def run(judge, jobs=1):
    """Each (system, condition) is its own process; up to `jobs` at once.
    A run writes its records only when it finishes, so a run cut short
    leaves nothing and is redone."""
    from concurrent.futures import ThreadPoolExecutor

    sources = json.loads((HERE / "sources.local.json").read_text())
    todo = [t for t in tasks() if not record_file(t[0], t[1], judge)[0].exists()]
    print(f"{judge}: {len(tasks()) - len(todo)} done, {len(todo)} to run, {jobs} at a time", flush=True)
    with ThreadPoolExecutor(max_workers=jobs) as pool:
        for line in pool.map(lambda t: one(judge, t[0], t[1], sources), todo):
            print(line, flush=True)


def one(judge, system, cond, sources):
    script, sysmdl, bp = next((s[1], s[2], s[3]) for s in SYSTEMS if s[0] == system)
    extra = [] if judge == "jev" else [f"--judge={judge}"]
    if script == "jabref":
        cmd = ["uv", "run", "-q", "--with", "xlrd", "python", "layer2.py", "meaning", "--no-roles",
               "--framing=arch", "--records"] + extra
        cwd = EVALS / "jabref"
    else:
        cmd = ["python3", "sysmdl_eval.py", sysmdl, *sources[system], f"--names={cond}",
               f"--blueprint={bp}", "--records"] + extra
        cwd = EVALS / "sysmdl"
    r = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True)
    return f"{system:15} {cond:8} exit {r.returncode}" + ("" if r.returncode == 0 else f"  {(r.stdout + r.stderr)[-300:]}")


def status(judges):
    total = len(tasks())
    flight = subprocess.run(["pgrep", "-fl", "onto discover"], capture_output=True, text=True).stdout
    parts = []
    for j in judges:
        done = sum(record_file(s, c, j)[0].exists() for s, c in tasks())
        running = flight.count(j) if j != "jev" else 0
        parts.append(f"{j.split('/')[-1][:28]} {done}/{total}" + (f" (+{running} running)" if running else ""))
    print(" | ".join(parts), flush=True)


def load(judge):
    """(system, cond, object) -> (right, p, input tokens, output tokens)."""
    out = {}
    for system, _, _, _, conds in SYSTEMS:
        for cond in conds:
            rec, gold = record_file(system, cond, judge)
            if not rec.exists():
                continue
            truth = {json.loads(l)["object"]: json.loads(l)["target"] for l in gold.read_text().splitlines()}
            for line in rec.read_text().splitlines():
                r = json.loads(line)
                if r["object"] not in truth:
                    continue
                t = r.get("tokens") or {}
                right = r["target"] is not None and r["target"] == truth[r["object"]] and r["disposition"] == "adopted"
                out[(system, cond, r["object"])] = (right, r["p"] or 0.0, t.get("input", 0), t.get("output", 0))
    return out


def mcnemar(a, b, keys):
    """Exact two-sided McNemar p for paired right/wrong on the same keys."""
    b01 = sum(1 for k in keys if a[k][0] and not b[k][0])
    b10 = sum(1 for k in keys if b[k][0] and not a[k][0])
    n = b01 + b10
    if n == 0:
        return 1.0, b01, b10
    k = min(b01, b10)
    p = sum(math.comb(n, i) for i in range(k + 1)) / 2 ** n * 2
    return min(1.0, p), b01, b10


def table(prices):
    judges = sorted({p.name.split("-j-")[1].removesuffix(".records.jsonl") if "-j-" in p.name else "jev"
                     for p in RECORDS.glob("*.records.jsonl")}
                    | {p.name.split("-judge-")[1].removesuffix(".records.jsonl")
                       for p in RECORDS.glob("JabRef-*-judge-*.records.jsonl")})
    data = {j: load(j if j == "jev" else j) for j in judges}
    # A judge's slug is what the files carry; compare on decisions every judge has.
    common = set.intersection(*(set(d) for d in data.values())) if data else set()
    print(f"{len(common)} decisions shared by {len(judges)} judges "
          f"({sum(1 for k in common if k[0] in SEALED)} on sealed systems)\n")
    base = data.get("jev")
    print(f"{'judge':48} {'acc all':>7} {'hidden':>7} {'visible':>7} {'Brier':>6} {'ECE':>6} "
          f"{'USD/1k':>7} {'acc/USD':>8} {'vs Jev p':>9}")
    rows = []
    for j in judges:
        d = data[j]
        ks = sorted(common)
        acc = lambda sel: (lambda v: sum(d[k][0] for k in v) / len(v) if v else float('nan'))([k for k in ks if sel(k)])
        q = [(d[k][1], d[k][0]) for k in ks]
        brier = sum((p - y) ** 2 for p, y in q) / len(q)
        bins = defaultdict(list)
        for p, y in q:
            bins[min(int(p * 5), 4)].append((p, y))
        ece = sum(len(v) / len(q) * abs(sum(y for _, y in v) / len(v) - sum(p for p, _ in v) / len(v)) for v in bins.values())
        price = prices.get(j)
        cost = (sum(d[k][2] for k in ks) * price[0] + sum(d[k][3] for k in ks) * price[1]) / 1e6 / len(ks) * 1000 if price else None
        a_all = acc(lambda k: True)
        p_vs = mcnemar(d, base, ks)[0] if base and j != "jev" else None
        rows.append((a_all, j))
        print(f"{j:48} {a_all:7.3f} {acc(lambda k: k[1] == 'hidden'):7.3f} {acc(lambda k: k[1] == 'visible'):7.3f} "
              f"{brier:6.3f} {ece:6.3f} {('%7.2f' % cost) if cost is not None else '      ?'} "
              f"{('%8.2f' % (a_all / cost)) if cost else '       ?'} {('%9.4f' % p_vs) if p_vs is not None else '        -'}")


def main():
    if sys.argv[1] == "run":
        jobs = next((int(a.split("=")[1]) for a in sys.argv[3:] if a.startswith("--jobs=")), 1)
        run(sys.argv[2], jobs)
    elif sys.argv[1] == "status":
        status(sys.argv[2:] or ["jev"])
    else:
        prices = {}
        for a in sys.argv[2:]:
            if a.startswith("--price="):
                j, v = a.split("=", 1)[1].rsplit("=", 1)
                prices[j] = tuple(float(x) for x in v.split(","))
        table(prices)


if __name__ == "__main__":
    main()
