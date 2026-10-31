"""onto discover against the expert mappings Olsson et al. used
(s4rdm3x `.sysmdl`: modules, a regex per module, allowed relations).

    python sysmdl_eval.py SYSTEM.sysmdl SRC_DIR [SRC_DIR ...] [--names=visible|hidden]
        [--blueprint=SYSTEM.blueprint.json] [--rescore] [--rep=N] [--records]
        [--sample=alpha|referenced] [--judge=MODEL] [--brief]   (MODEL as onto discover --judge-model)

--records also writes ../calibration/records/TAG.records.jsonl (onto
discover --records) and TAG.gold.jsonl (the expert's majority module per
package, as answers) for the calibration study.

--blueprint gives module descriptions (the system's documentation,
restated); without it a module is described by its name.

Conditions:
- names=visible: each package is described by its path and class names.
- names=hidden:  packages get shuffled anonymous ids; only class names
  remain. This simulates a system whose package names do not mirror its
  architecture: in 7 of the 8 benchmark systems the expert mapping is a
  package-prefix rule, so with paths visible the answer is in the input.
Also reported: a string baseline that maps a package to the module whose
name tokens it shares most, without any judge.

Graded at class level against the regex mapping (first matching rule),
P over mapped classes, R over all (Olsson et al.), macro F1, coverage,
the package-level oracle, and calibration of the judge's p (Brier, ECE,
Platt leave-one-out).
"""

import json
import os
import math
import random
import re
import subprocess
import sys
from collections import Counter, defaultdict
from pathlib import Path

HERE = Path(__file__).parent
ONTO = os.environ.get("ONTO_BIN", str(HERE.parents[3] / "target/release/onto"))
FRAMING = {
    "goal": "Recover the intended architecture of {src}: find the module of {dst} that the code unit {object} belongs to.",
    "instructions": "Which module of {dst} is responsible for this code unit? Judge by its responsibility, as its name and class names show, against each module's name and description. If none fits, answer none of these.",
}


def opt(name, default):
    for a in sys.argv[1:]:
        if a.startswith(f"--{name}="):
            return a.split("=", 1)[1]
    return default


def parse_sysmdl(path):
    sec, modules, rules, relations = None, [], [], []
    for line in Path(path).read_text().splitlines():
        s = line.strip()
        head = re.match(r"#\s*(modules|mapping|relations|jar|name|root-packages|metrics file)\s*$", s)
        if head:
            sec = head.group(1)
            continue
        s = s.split("\t#")[0].strip()
        if not s or s.startswith("#"):
            continue
        if sec == "modules":
            modules.append(s)
        elif sec == "mapping":
            m, rx = s.split(None, 1)
            rules.append((m, re.compile(rx)))
        elif sec == "relations":
            a, b = s.split()
            relations.append((a, b))
    return modules, rules, relations


IMPORT = re.compile(r"^\s*import\s+(static\s+)?([\w.]+)(\.\*)?\s*;", re.M)
PACKAGE = re.compile(r"^\s*package\s+([\w.]+)\s*;", re.M)
STRIP = re.compile(r'//[^\n]*|/\*.*?\*/|"(?:\\.|[^"\\])*"|\'(?:\\.|[^\'\\])*\'', re.S)
QUALIFIED = re.compile(r"\b((?:[a-z]\w*\.)+[A-Z]\w*)")
SIMPLE = re.compile(r"\b([A-Z]\w*)\b")


def top_level(fqn):
    parts = fqn.split(".")
    for i, p in enumerate(parts):
        if p[:1].isupper():
            return ".".join(parts[: i + 1])
    return fqn


def read_sources(dirs):
    """class FQN -> referenced project classes (imports, qualified names,
    simple names resolved against imports and the own package)."""
    classes = {}
    for d in dirs:
        for f in sorted(Path(d).rglob("*.java")):
            text = f.read_text(errors="replace")
            pkg = PACKAGE.search(text)
            pkg = pkg.group(1) if pkg else ""
            classes[f"{pkg}.{f.stem}" if pkg else f.stem] = (pkg, text)
    by_package = defaultdict(dict)
    for fqn, (pkg, _) in classes.items():
        by_package[pkg][fqn.rsplit(".", 1)[-1]] = fqn
    deps = {}
    for fqn, (pkg, text) in classes.items():
        explicit, wildcards, refs = {}, [pkg], set()
        for static, name, star in IMPORT.findall(text):
            if star:
                wildcards.append(name)
                continue
            target = top_level(name.rsplit(".", 1)[0] if static else name)
            explicit[target.rsplit(".", 1)[-1]] = target
            if target in classes:
                refs.add(target)
        body = STRIP.sub(" ", IMPORT.sub(" ", PACKAGE.sub(" ", text)))
        for q in QUALIFIED.findall(body):
            if top_level(q) in classes:
                refs.add(top_level(q))
        for simple in set(SIMPLE.findall(body)):
            if simple in explicit:
                if explicit[simple] in classes:
                    refs.add(explicit[simple])
                continue
            for w in wildcards:
                hit = by_package.get(w, {}).get(simple)
                if hit:
                    refs.add(hit)
                    break
        refs.discard(fqn)
        deps[fqn] = refs
    return deps


def sample(classes, referenced, k=15):
    """The class names a unit is described by. alpha (the default, and
    every run before 2026-10-31): the alphabetical head, which for a
    large unit can be all of one kind (Ant's tasks start with Abstract*,
    Ant*). referenced: the classes other units reference most, ties
    broken alphabetically, then the rest evenly spaced."""
    names = sorted(classes)
    if opt("sample", "alpha") != "referenced" or len(names) <= k:
        return [c.rsplit(".", 1)[1] for c in names[:k]]
    used = [c for c in sorted(names, key=lambda c: (-referenced[c], c)) if referenced[c]][: k * 2 // 3]
    rest = [c for c in names if c not in used]
    step = max(1, len(rest) // (k - len(used))) if len(used) < k else 1
    picked = used + rest[::step][: k - len(used)]
    return [c.rsplit(".", 1)[1] for c in picked]


def module_of(cls, rules):
    for m, rx in rules:
        if rx.search(cls):
            return m
    return None


def tokens(s):
    parts = re.split(r"[._\-]|(?<=[a-z])(?=[A-Z])", s)
    return {p.lower() for p in parts if len(p) > 2}


def score(pred, truth, modules):
    """Single-label mapping with abstention. Micro P (over mapped) and R
    (over all) are Olsson et al.'s; at full coverage both equal accuracy
    (every error is one FP and one FN), so the report leads with
    accuracy and coverage, and carries per-module P/R, macro P/R/F1 and
    the largest confusions, where the errors actually show."""
    correct = sum(pred[c] == truth[c] for c in pred)
    P = correct / len(pred) if pred else 0
    R = correct / len(truth)
    per, ps, rs, f1s = {}, [], [], []
    for m in modules:
        gold = {c for c, t in truth.items() if t == m}
        if not gold:
            continue
        got = {c for c, t in pred.items() if t == m}
        tp = len(gold & got)
        p = tp / len(got) if got else 0
        r = tp / len(gold)
        f = 2 * p * r / (p + r) if p + r else 0
        per[m] = dict(n=len(gold), P=round(p, 3), R=round(r, 3), F1=round(f, 3))
        ps.append(p)
        rs.append(r)
        f1s.append(f)
    confusions = Counter((truth[c], pred[c]) for c in pred if pred[c] != truth[c])
    return dict(
        accuracy=round(correct / len(truth), 3), coverage=round(len(pred) / len(truth), 3),
        P=round(P, 3), R=round(R, 3), F1=round(2 * P * R / (P + R), 3) if P + R else 0,
        macro_P=round(sum(ps) / len(ps), 3), macro_R=round(sum(rs) / len(rs), 3),
        macro_F1=round(sum(f1s) / len(f1s), 3), per_module=per,
        confusions=[f"{t} -> {p}: {n}" for (t, p), n in confusions.most_common(5)],
    )


def calibration(points):
    def logit(p):
        p = min(max(p, 1e-4), 1 - 1e-4)
        return math.log(p / (1 - p))

    def fit(pts):
        a, b = 1.0, 0.0
        for _ in range(2000):
            ga = gb = 0.0
            for p, y in pts:
                q = 1 / (1 + math.exp(-(a * logit(p) + b)))
                ga += (q - y) * logit(p)
                gb += q - y
            a -= 0.05 * ga / len(pts)
            b -= 0.05 * gb / len(pts)
        return a, b

    def stats(pairs):
        bins = defaultdict(list)
        for p, y in pairs:
            bins[min(int(p * 5), 4)].append((p, y))
        return dict(
            brier=round(sum((p - y) ** 2 for p, y in pairs) / len(pairs), 3),
            ece_5bins=round(sum(len(b) / len(pairs) * abs(sum(y for _, y in b) / len(b) - sum(p for p, _ in b) / len(b))
                                for b in bins.values()), 3),
        )

    pts = [(p, float(y)) for p, y in points]
    if len(pts) < 3:
        return None
    loo = []
    for i, (p, y) in enumerate(pts):
        a, b = fit(pts[:i] + pts[i + 1:])
        loo.append((1 / (1 + math.exp(-(a * logit(p) + b))), y))
    return dict(points=len(pts), raw=stats(pts), platt_loo=stats(loo))


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    sysmdl, src_dirs = Path(args[0]), args[1:]
    names = opt("names", "visible")
    name = re.sub(r"\W", "", sysmdl.stem.split("_")[0].split(".")[0]).capitalize()
    blueprint = opt("blueprint", "")
    tag = (f"{name}-{names}" + (f"-bp-{Path(blueprint).stem.split('.')[-1]}" if blueprint else "")
           + ("-sref" if opt("sample", "alpha") == "referenced" else "")
           + (f"-j-{re.sub(r'[^A-Za-z0-9.]+', '_', opt('judge', ''))}" if opt("judge", "") else "")
           + (f"-casc-{re.sub(r'[^A-Za-z0-9.]+', '_', opt('escalate', ''))}-{opt('escalate-below', '0.7')}"
              if opt("escalate", "") else "")
           + (f"-rep-{opt('rep', '1')}" if opt("rep", "1") != "1" else "")
           + ("-brief" if "--brief" in sys.argv else ""))
    described = json.loads(Path(blueprint).read_text()) if blueprint else {}
    modules, rules, relations = parse_sysmdl(sysmdl)

    deps = read_sources(src_dirs)
    truth = {c: module_of(c, rules) for c in deps}
    truth = {c: m for c, m in truth.items() if m}
    pkg_of = {c: c.rsplit(".", 1)[0] for c in deps}
    classes_in = defaultdict(list)
    for c, p in pkg_of.items():
        classes_in[p].append(c)
    pkgs = sorted(classes_in)
    ids = list(range(len(pkgs)))
    random.Random(7).shuffle(ids)
    ident = {p: (f"p_{re.sub(r'[^a-z0-9]+', '_', p.lower())}" if names == "visible" else f"u{ids[i]:03d}")
             for i, p in enumerate(pkgs)}
    back = {v: k for k, v in ident.items()}
    mid = {m: "m_" + re.sub(r"[^A-Za-z0-9]+", "_", m) for m in modules}
    mback = {v: k for k, v in mid.items()}

    # How often each class is referenced from other packages: the classes
    # a unit is known by (--sample=referenced).
    referenced = Counter(r for c, refs in deps.items() for r in refs if pkg_of[r] != pkg_of[c])
    edges = Counter()
    for c, refs in deps.items():
        for r in refs:
            if pkg_of[r] != pkg_of[c]:
                edges[(pkg_of[c], pkg_of[r])] += 1
    lines = [f"category {name}Code {{", f"    objects: {', '.join(ident[p] for p in pkgs)};"]
    for p in pkgs:
        cls = ", ".join(sample(classes_in[p], referenced))
        if opt("sample", "alpha") == "referenced":
            size = f"{len(classes_in[p])} classes, the most used of them"
            about = f"package {p}: {size}: {cls}" if names == "visible" else f"a code unit of {size}: {cls}"
        else:
            about = f"package {p}: classes {cls}" if names == "visible" else f"a code unit with classes {cls}"
        lines.append(f'    about {ident[p]}: "{about}";')
    for (a, b), n in sorted(edges.items()):
        lines.append(f'    d_{ident[a]}__{ident[b]}: {ident[a]} -> {ident[b]} "{n} references";')
    lines += ["}", "", "category Intended {", f"    objects: {', '.join(mid.values())};"]
    for m in modules:
        parts = m.split(".")
        about = described.get(m) or (f"module {m}" + (f": the {parts[-1]} part of {'.'.join(parts[:-1])}"
                                                      if len(parts) > 1 else ""))
        lines.append(f'    about {mid[m]}: "{about}";')
    for a in modules:  # complete target: map by meaning, check with the functor later
        for b in modules:
            if a != b:
                lines.append(f"    a_{mid[a]}__{mid[b]}: {mid[a]} -> {mid[b]};")
    lines.append("}")
    path = HERE / f"l2-{tag}.onto"
    path.write_text("\n".join(lines) + "\n")

    saved = HERE / f"l2-{tag}.out"
    rec_dir = HERE.parent / "calibration" / "records"
    records = rec_dir / f"{tag}.records.jsonl" if "--records" in sys.argv else None
    if "--rescore" in sys.argv and saved.exists():
        out = saved.read_text()
    else:
        if records:
            rec_dir.mkdir(parents=True, exist_ok=True)
        run = subprocess.run(
            [ONTO, "discover", f"{path}#{name}Code", f"{path}#Intended", "--non-injective", "--no-roles",
             "--name", "Proposed", "--goal", FRAMING["goal"], "--instructions", FRAMING["instructions"]]
            + (["--records", str(records)] if records else [])
            + (["--judge-model", opt("judge", "")] if opt("judge", "") else [])
            + (["--brief"] if "--brief" in sys.argv else [])
            + (["--escalate-to", opt("escalate", ""), "--escalate-below", opt("escalate-below", "0.7")]
               if opt("escalate", "") else []),
            capture_output=True, text=True, timeout=3600)
        out = run.stdout + run.stderr
        saved.write_text(out)
        if run.returncode:
            sys.exit(out[-1500:])

    mapped = {}
    for m in re.finditer(r"^\s+(\w+) -> (m_\w+)[,;]\s+#\s*(.*)$", out, re.M):
        p = re.search(r"judged ([\d.]+)", m.group(3))
        mapped[back[m.group(1)]] = (mback[m.group(2)], float(p.group(1)) if p else None)

    disc = {c: mapped[pkg_of[c]][0] for c in truth if pkg_of[c] in mapped}
    string = {}
    for c in truth:
        best = max(modules, key=lambda m: (len(tokens(m) & tokens(pkg_of[c])), -len(tokens(m))))
        if tokens(best) & tokens(pkg_of[c]):
            string[c] = best
    points = []
    for p, cls in classes_in.items():
        gold = Counter(truth[c] for c in cls if c in truth)
        if gold and p in mapped and mapped[p][1] is not None:
            points.append((mapped[p][1], mapped[p][0] == gold.most_common(1)[0][0]))
    by_pkg = defaultdict(Counter)
    for c, m in truth.items():
        by_pkg[pkg_of[c]][m] += 1
    if records:
        with open(rec_dir / f"{tag}.gold.jsonl", "w") as g:
            for p, counts in sorted(by_pkg.items()):
                g.write(json.dumps({"object": ident[p], "target": mid[counts.most_common(1)[0][0]],
                                    "classes": sum(counts.values())}) + "\n")
    report = dict(
        run=tag, classes=len(truth), classes_read=len(deps), packages=len(pkgs), modules=len(modules),
        oracle_package_level=round(sum(c.most_common(1)[0][1] for c in by_pkg.values()) / len(truth), 3),
        discover=score(disc, truth, modules),
        string_baseline=score(string, truth, modules),
        calibration=calibration(points),
        failed=(re.search(r"judge failed for (\d+)", out) or [None, "0"])[1],
    )
    (HERE / f"l2-{tag}.json").write_text(json.dumps(report, indent=1))
    print(json.dumps(report, indent=1))


if __name__ == "__main__":
    main()
