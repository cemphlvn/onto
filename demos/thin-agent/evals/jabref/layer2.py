"""Layer 2: onto discover proposes the code -> module mapping; graded
against the expert mapping, as in Olsson et al. (P over mapped, R over
all entities), plus coverage and calibration of the judge's p.

General: source = packages with dependencies read from code, described
only by code (package name, class names); target = modules described by
the system's own documentation, verbatim; grading at the expert's
entity level. System-specific: everything in SYSTEM.

    python layer2.py structure|meaning
"""

import json
import os
import re
import subprocess
import sys
from collections import Counter, defaultdict
from pathlib import Path

import jabref_eval as je

HERE = Path(__file__).parent
ONTO = os.environ.get("ONTO_BIN", str(HERE.parents[3] / "target/release/onto"))

# The system's own words (JabRef wiki, High Level Documentation, 2016).
SYSTEM = {
    "name": "JabRef",
    "prefix": "net.sf.jabref",
    # The documentation's package rule: "For each layer, we form
    # packages"; "All packages and classes which are currently not part
    # of these packages ... are considered as gui classes".
    "doc_packages": {"net.sf.jabref.gui": "GUI", "net.sf.jabref.logic": "Logic",
                     "net.sf.jabref.model": "Model", "net.sf.jabref.cli": "CommandLineInterface",
                     "net.sf.jabref.preferences": "Pref"},
    "doc_default": "GUI",
    "threshold": 0.4,
    "modules": {
        "GUI": "the gui is the outer shell; only the gui knows the user and his preferences, and can interact with him to help him solve tasks. All packages and classes which are currently not part of these packages are considered as gui classes",
        "Logic": "the logic is responsible for reading/writing/importing/exporting and manipulating the model, and it is structured often as an API the gui can call and use; an intermediate layer towards the gui",
        "Model": "the model represents the most important data structures (BibDatabases, BibEntries, Events, and related aspects) and has only a little bit of logic attached; the model is in the center",
        "CommandLineInterface": "the cli package bundles classes that are responsible for JabRef's command line interface",
        "Pref": "the preferences represents all information customizable by a user for her personal needs",
        "Globals": "global classes, used everywhere",
    },
}


ARCH_FRAMING = {
    "goal": "Recover the intended architecture of {src}: find the module of {dst} that the code unit {object} belongs to.",
    "instructions": "Which module of {dst} is responsible for this code unit? Judge by its responsibility, as its package name and class names show, against each module's description. If none fits, answer none of these.",
}


def opt(name, default):
    for a in sys.argv[1:]:
        if a.startswith(f"--{name}="):
            return a.split("=", 1)[1]
    return default


def ident(pkg):
    rest = pkg[len(SYSTEM["prefix"]):].strip(".")
    return "p_" + (rest.replace(".", "_") or "root")


def build(mode):
    deps = je.read_sources()
    units = je.load_mapping()
    forbidden = je.load_rules()
    pkg_of = {c: c.rsplit(".", 1)[0] for c in deps}
    classes_in = defaultdict(list)
    for c, p in pkg_of.items():
        classes_in[p].append(c)
    edges = Counter()
    for c, refs in deps.items():
        for r in refs:
            if r in pkg_of and pkg_of[r] != pkg_of[c]:
                edges[(pkg_of[c], pkg_of[r])] += 1

    lines = [f"category {SYSTEM['name']}Code {{",
             f"    objects: {', '.join(ident(p) for p in sorted(classes_in))};"]
    for p in sorted(classes_in):
        names = ", ".join(sorted(c.rsplit('.', 1)[1] for c in classes_in[p])[:15])
        if opt("source", "classes") == "classes":
            lines.append(f'    about {ident(p)}: "package {p}: classes {names}";')
        else:
            lines.append(f'    about {ident(p)}: "package {p}";')
    for (a, b), n in sorted(edges.items()):
        lines.append(f'    d_{ident(a)}__{ident(b)}: {ident(a)} -> {ident(b)} "{n} references";')
    lines.append("}\n")

    mods = list(SYSTEM["modules"])
    lines.append("category Intended {")
    lines.append(f"    objects: {', '.join(mods)};")
    if opt("target", "doc") == "doc":
        for m, text in SYSTEM["modules"].items():
            lines.append(f'    about {m}: "{text}";')
    for a in mods:
        for b in mods:
            if a != b and (mode == "meaning" or (a, b) not in forbidden):
                lines.append(f"    a_{a}__{b}: {a} -> {b};")
    lines.append("}")
    path = HERE / f"l2-{tag()}.onto"
    path.write_text("\n".join(lines) + "\n")
    return path, deps, units, pkg_of, classes_in, len(edges)


def parse(out):
    mapped, outside = {}, {}
    for m in re.finditer(r"^\s+(p_\w+) -> (\w+)[,;]\s+#\s*(.*)$", out, re.M):
        p = re.search(r"judged ([\d.]+)", m.group(3))
        mapped[m.group(1)] = (m.group(2), float(p.group(1)) if p else None)
    o = re.search(r"outside the domain: (.*)", out)
    if o:
        for item in o.group(1).split(", p_"):
            m = re.match(r"(?:p_)?(\w+)(?: \(none of these ([\d.]+)\))?", item)
            outside["p_" + m.group(1)] = float(m.group(2)) if m.group(2) else None
    return mapped, outside


def ceiling(truth, pkg_of):
    """Best P = R a package-level mapping can reach: every package to its
    expert majority module."""
    by_pkg = defaultdict(Counter)
    for c, m in truth.items():
        by_pkg[pkg_of[c]][m] += 1
    return round(sum(c.most_common(1)[0][1] for c in by_pkg.values()) / len(truth), 3)


def score(pred, truth):
    """P over mapped classes, R over all (Olsson et al.), macro F1."""
    correct = sum(pred[c] == truth[c] for c in pred)
    P = correct / len(pred) if pred else 0
    R = correct / len(truth)
    per = {}
    for m in SYSTEM["modules"]:
        gold = {c for c, t in truth.items() if t == m}
        got = {c for c, t in pred.items() if t == m}
        tp = len(gold & got)
        p = tp / len(got) if got else 0
        r = tp / len(gold) if gold else 0
        per[m] = dict(n=len(gold), P=round(p, 3), R=round(r, 3),
                      F1=round(2 * p * r / (p + r), 3) if p + r else 0)
    live = [v["F1"] for v in per.values() if v["n"]]
    return dict(coverage=round(len(pred) / len(truth), 3), P=round(P, 3), R=round(R, 3),
                F1=round(2 * P * R / (P + R), 3) if P + R else 0,
                macro_F1=round(sum(live) / len(live), 3), per_module=per)


def doc_rule(pkg):
    """The documentation's naming rule for a package, or None."""
    for prefix, module in SYSTEM["doc_packages"].items():
        if pkg == prefix or pkg.startswith(prefix + "."):
            return module
    return None


def calibration(points):
    """Brier and 5-bin ECE of raw p, and of p after Platt scaling fit
    leave-one-out (logistic on logit p)."""
    import math

    def logit(p):
        p = min(max(p, 1e-4), 1 - 1e-4)
        return math.log(p / (1 - p))

    def fit(pts):
        a, b = 1.0, 0.0
        for _ in range(3000):
            ga = gb = 0.0
            for p, y in pts:
                q = 1 / (1 + math.exp(-(a * logit(p) + b)))
                ga += (q - y) * logit(p)
                gb += q - y
            a -= 0.05 * ga / len(pts)
            b -= 0.05 * gb / len(pts)
        return a, b

    def stats(pairs):
        brier = sum((p - y) ** 2 for p, y in pairs) / len(pairs)
        bins = defaultdict(list)
        for p, y in pairs:
            bins[min(int(p * 5), 4)].append((p, y))
        ece = sum(len(b) / len(pairs) * abs(sum(y for _, y in b) / len(b) - sum(p for p, _ in b) / len(b))
                  for b in bins.values())
        return dict(brier=round(brier, 3), ece_5bins=round(ece, 3),
                    bins={f"{k/5:.1f}-{(k+1)/5:.1f}": [len(v), round(sum(y for _, y in v) / len(v), 2)]
                          for k, v in sorted(bins.items())})

    pts = [(p, float(y)) for p, y in points]
    loo = []
    for i, (p, y) in enumerate(pts):
        a, b = fit(pts[:i] + pts[i + 1:])
        loo.append((1 / (1 + math.exp(-(a * logit(p) + b))), y))
    return dict(points=len(pts), raw=stats(pts), platt_loo=stats(loo))


def tag():
    mode = next((a for a in sys.argv[1:] if not a.startswith("--")), "structure")
    parts = [mode] + (["noroles"] if "--no-roles" in sys.argv else [])
    for name, default in (("framing", "default"), ("target", "doc"), ("source", "classes"), ("rep", "1"), ("judge", "")):
        if opt(name, default) != default:
            parts.append(f"{name}-{re.sub(r'[^A-Za-z0-9.]+', '_', opt(name, default))}")
    return "-".join(parts)


def main(mode):
    roles = "--no-roles" in sys.argv
    tag_ = tag()
    path, deps, units, pkg_of, classes_in, n_edges = build(mode)
    print(f"{tag_}: {len(classes_in)} packages, {n_edges} package dependencies")
    saved = HERE / f"l2-{tag_}.out"
    rec_dir = HERE.parent / "calibration" / "records"
    records = rec_dir / f"JabRef-{tag_}.records.jsonl" if "--records" in sys.argv else None
    if records:
        rec_dir.mkdir(parents=True, exist_ok=True)
    if "--rescore" in sys.argv and saved.exists():
        run = subprocess.CompletedProcess([], 0, saved.read_text(), "")
    else:
        run = subprocess.run(
            [ONTO, "discover", f"{path}#{SYSTEM['name']}Code", f"{path}#Intended",
             "--non-injective", "--name", "Proposed"] + (["--no-roles"] if roles else [])
            + (["--goal", ARCH_FRAMING["goal"], "--instructions", ARCH_FRAMING["instructions"]]
               if opt("framing", "default") == "arch" else [])
            + (["--records", str(records)] if records else [])
            + (["--judge-model", opt("judge", "")] if opt("judge", "") else []),
            capture_output=True, text=True, timeout=3600,
        )
        saved.write_text(run.stdout + run.stderr)
    if run.returncode:
        sys.exit(run.stderr[-2000:])
    mapped, outside = parse(run.stdout)

    truth = {c: je.module_of(c, units) for c in deps}
    truth = {c: m for c, m in truth.items() if m}
    pkg = lambda c: ident(pkg_of[c])
    disc = {c: mapped[pkg(c)][0] for c in truth if pkg(c) in mapped}
    sure = {c: mapped[pkg(c)][0] for c in truth
            if pkg(c) in mapped and (mapped[pkg(c)][1] is None or mapped[pkg(c)][1] >= SYSTEM["threshold"])}
    rules = {c: doc_rule(pkg_of[c]) or SYSTEM["doc_default"] for c in truth}
    pinned = {c: doc_rule(pkg_of[c]) for c in truth if doc_rule(pkg_of[c])}
    doc_disc = {c: pinned.get(c) or disc.get(c) or SYSTEM["doc_default"] for c in truth}
    doc_sure = {c: pinned.get(c) or sure.get(c) or SYSTEM["doc_default"] for c in truth}

    if records:
        with open(rec_dir / f"JabRef-{tag_}.gold.jsonl", "w") as g:
            for p, cls in sorted(classes_in.items()):
                gold = Counter(truth[c] for c in cls if c in truth)
                if gold:
                    g.write(json.dumps({"object": ident(p), "target": gold.most_common(1)[0][0],
                                        "classes": sum(gold.values())}) + "\n")
    points = []
    for p, cls in classes_in.items():
        gold = Counter(truth[c] for c in cls if c in truth)
        if gold and ident(p) in mapped and mapped[ident(p)][1] is not None:
            module, prob = mapped[ident(p)]
            points.append((prob, module == gold.most_common(1)[0][0]))

    report = dict(
        run=tag_, classes=len(truth), packages=len(classes_in), packages_mapped=len(mapped),
        packages_outside=len(outside),
        search=(re.search(r"^search: (.*)", run.stdout, re.M) or [None, None])[1],
        oracle_package_level=ceiling(truth, pkg_of),
        variants={
            "discover": score(disc, truth),
            f"discover, p >= {SYSTEM['threshold']} (abstain below)": score(sure, truth),
            "documentation rules only": score(rules, truth),
            "doc rules pinned + discover + doc default": score(doc_disc, truth),
            f"doc rules pinned + discover p >= {SYSTEM['threshold']} + doc default": score(doc_sure, truth),
        },
        pinned_classes=len(pinned),
        calibration=calibration(points) if points else None,
    )
    (HERE / f"l2-{tag_}.json").write_text(json.dumps(report, indent=1))
    print(json.dumps({k: v for k, v in report.items() if k != "variants"}, indent=1))
    for name, v in report["variants"].items():
        print(f"{name:60} cov {v['coverage']:.3f}  P {v['P']:.3f}  R {v['R']:.3f}  F1 {v['F1']:.3f}  macro {v['macro_F1']:.3f}")


if __name__ == "__main__":
    main(next((a for a in sys.argv[1:] if not a.startswith("--")), "structure"))
