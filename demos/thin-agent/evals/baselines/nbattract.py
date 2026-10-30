"""NBAttract (Olsson, Ericsson, Wingkvist, JSS; arXiv 2109.09525),
re-implemented and run on the same input onto's discover sees, so the two
can be compared under one protocol.

    uv run --with numpy --with scikit-learn --with nltk python nbattract.py [--reps=50] [--system=NAME] [--input=onto|native]

Input per class: its name (and, names visible, its package path), split
on camel case, Porter-stemmed, terms of three letters or more; plus CDA
terms (`<module>-uses-<module>`) from its dependencies on mapped classes,
generated as if the class sat in each candidate module. Olsson et al.
also used method, variable and string names from compiled code; onto
sees class names only, so NBAttract here does too.

Method (HuGMe with NBAttract): one document per module, the set of terms
of its mapped classes plus the module's name; multinomial naive Bayes on
term occurrence; an orphan is mapped to the module whose hypothetical
document gives it the highest probability, when that probability is at
least 0.9; repeated until nothing more maps. P over mapped orphans, R
over all orphans, F1; medians over random initial sets, each with at
least one class per module (as Olsson et al.).
"""

import json
import random
import re
import sys
from collections import defaultdict
from pathlib import Path

import numpy as np
from nltk.stem import PorterStemmer

HERE = Path(__file__).parent
EVALS = HERE.parent
sys.path.insert(0, str(EVALS / "sysmdl"))
import sysmdl_eval as se  # noqa: E402

SIZES = (0.0, 0.01, 0.05, 0.10, 0.20)
# onto: class names (and package paths) only, the judge's input; native:
# every identifier and string in the class, NBAttract's own input.
INPUT = next((a.split("=")[1] for a in sys.argv[1:] if a.startswith("--input=")), "onto")
THRESHOLD = 0.9
STEM = PorterStemmer().stem
SYSTEMS = [  # name, sysmdl, sources key
    ("Teammates", "teammates.sysmdl"), ("Argouml", "argouml.sysmdl"), ("Prom", "ProM_6_9.sysmdl"),
    ("Lucene", "lucene.sysmdl"), ("Commonsimaging", "commons-imaging.sysmdl"), ("Ant", "ant.sysmdl"),
    ("Sweethome3d", "sweethome3d.sysmdl"),
]


def terms(text):
    parts = re.split(r"[^A-Za-z0-9]+|(?<=[a-z0-9])(?=[A-Z])|(?<=[A-Z])(?=[A-Z][a-z])", text)
    return {STEM(p.lower()) for p in parts if len(p) >= 3}


JAVA_KEYWORDS = set("""abstract assert boolean break byte case catch char class const continue default do
double else enum extends final finally float for goto if implements import instanceof int interface long
native new package private protected public return short static strictfp super switch synchronized this
throw throws transient try void volatile while true false null var record yield sealed permits""".split())
COMMENT = re.compile(r"//[^\n]*|/\*.*?\*/", re.S)
STRING = re.compile(r'"((?:\\.|[^"\\])*)"')
IDENT = re.compile(r"\b[A-Za-z_][A-Za-z0-9_]*\b")
QUALIFIED = re.compile(r"\b(?:[a-z][a-z0-9_]*\.)+(?=[A-Za-z])")


def native_terms(text, visible):
    """Olsson et al.'s term kinds, from source instead of bytecode: names of
    the class, its methods, fields, variables and parameters, and string
    constants; comments and keywords out. Names hidden: the package and
    import lines and every dotted package prefix are removed first."""
    text = COMMENT.sub(" ", text)
    if not visible:
        text = re.sub(r"^\s*(package|import)\b[^;]*;", " ", text, flags=re.M)
        text = QUALIFIED.sub(" ", text)
    strings = " ".join(STRING.findall(text))
    code = STRING.sub(" ", text)
    words = [w for w in IDENT.findall(code) if w not in JAVA_KEYWORDS] + strings.split()
    return set().union(*(terms(w) for w in words)) if words else set()


def sources_by_class(dirs):
    out = {}
    for d in dirs:
        for f in Path(d).rglob("*.java"):
            text = f.read_text(errors="replace")
            pkg = se.PACKAGE.search(text)
            out[f"{pkg.group(1)}.{f.stem}" if pkg else f.stem] = text
    return out


def load(system, sysmdl, sources):
    modules, rules, _ = se.parse_sysmdl(EVALS / "sysmdl" / "data" / sysmdl)
    deps = se.read_sources(sources[system])
    gold = {c: se.module_of(c, rules) for c in deps}
    gold = {c: m for c, m in gold.items() if m}
    deps = {c: {d for d in ds if d in gold} for c, ds in deps.items() if c in gold}
    text = sources_by_class(sources[system]) if INPUT == "native" else {}
    return modules, gold, deps, text


class Experiment:
    def __init__(self, modules, gold, deps, visible, text=None):
        self.modules = [m for m in modules if any(g == m for g in gold.values())] or modules
        self.all_modules = modules
        self.gold, self.deps = gold, deps
        self.incoming = defaultdict(set)
        for c, ds in deps.items():
            for d in ds:
                self.incoming[d].add(c)
        self.classes = sorted(gold)
        self.base = {}
        for c in self.classes:
            pkg, name = c.rsplit(".", 1)
            t = terms(name)
            if visible:
                t |= terms(pkg.replace(".", " "))
            if text and c in text:
                t |= native_terms(text[c], visible)
            self.base[c] = t
        self.name_terms = {m: terms(m.replace(".", " ")) for m in self.all_modules}

    def cda(self, c, m, where):
        """Dependency terms of class c as if it sat in module m, against
        classes already mapped (`where`)."""
        out = set()
        for d in self.deps[c]:
            if d in where:
                out.add(f"cda:{m}-uses-{where[d]}")
        for s in self.incoming[c]:
            if s in where:
                out.add(f"cda:{where[s]}-uses-{m}")
        return out

    def run(self, initial):
        where = dict(initial)
        while True:
            docs = {m: set(self.name_terms[m]) for m in self.all_modules}
            for c, m in where.items():
                docs[m] |= self.base[c] | self.cda(c, m, where)
            vocab = {t: i for i, t in enumerate(sorted(set().union(*docs.values())))}
            mods = self.all_modules
            counts = np.zeros((len(mods), len(vocab)))
            for k, m in enumerate(mods):
                for t in docs[m]:
                    counts[k, vocab[t]] = 1
            # Multinomial NB, Laplace smoothing, uniform prior over modules.
            flp = np.log((counts + 1) / (counts.sum(axis=1, keepdims=True) + len(vocab)))
            newly = {}
            for o in self.classes:
                if o in where:
                    continue
                best, best_p = None, 0.0
                for k, m in enumerate(mods):
                    idx = [vocab[t] for t in self.base[o] | self.cda(o, m, where) if t in vocab]
                    if not idx:
                        continue
                    ll = flp[:, idx].sum(axis=1)
                    p = np.exp(ll - ll.max())
                    p = p[k] / p.sum()
                    if p > best_p:
                        best, best_p = m, p
                if best is not None and best_p >= THRESHOLD:
                    newly[o] = best
            if not newly:
                return where
            where.update(newly)

    def score(self, initial):
        mapped = self.run(initial)
        orphans = [c for c in self.classes if c not in initial]
        got = [c for c in orphans if c in mapped]
        right = sum(mapped[c] == self.gold[c] for c in got)
        P = right / len(got) if got else 0.0
        R = right / len(orphans) if orphans else 0.0
        return P, R, (2 * P * R / (P + R) if P + R else 0.0), len(initial)

    def initial_set(self, frac, rng):
        if frac == 0:
            return {}
        by_mod = defaultdict(list)
        for c in self.classes:
            by_mod[self.gold[c]].append(c)
        pick = {rng.choice(v) for v in by_mod.values()}
        n = max(len(pick), round(frac * len(self.classes)))
        rest = [c for c in self.classes if c not in pick]
        rng.shuffle(rest)
        pick |= set(rest[: n - len(pick)])
        return {c: self.gold[c] for c in pick}


def main():
    """One system per call (--system=NAME), so systems run in parallel;
    both thresholds: Olsson et al.'s 0.9, and 0 (always map, argmax), which
    shows what the input carries when the 0.9 rule maps nothing."""
    global THRESHOLD
    reps = next((int(a.split("=")[1]) for a in sys.argv[1:] if a.startswith("--reps=")), 50)
    only = next((a.split("=")[1] for a in sys.argv[1:] if a.startswith("--system=")), None)
    sources = json.loads((EVALS / "calibration" / "sources.local.json").read_text())
    out = {}
    for system, sysmdl in SYSTEMS:
        if only and system != only:
            continue
        modules, gold, deps, text = load(system, sysmdl, sources)
        for names, thr in [(n, t) for n in ("visible", "hidden") for t in (0.9, 0.0)]:
            THRESHOLD = thr
            exp = Experiment(modules, gold, deps, names == "visible", text)
            row = {}
            for frac in SIZES:
                rng = random.Random(f"{system}-{names}-{frac}")
                runs = [exp.score(exp.initial_set(frac, rng)) for _ in range(1 if frac == 0 else reps)]
                arr = np.array(runs)
                row[frac] = dict(P=float(np.median(arr[:, 0])), R=float(np.median(arr[:, 1])),
                                 F1=float(np.median(arr[:, 2])), F1_q25=float(np.quantile(arr[:, 2], 0.25)),
                                 F1_q75=float(np.quantile(arr[:, 2], 0.75)), labelled=float(np.median(arr[:, 3])),
                                 classes=len(exp.classes))
                print(f"{system:15} {names:8} t={thr} {frac:5.0%}  labelled {row[frac]['labelled']:5.0f}/{len(exp.classes):<5}"
                      f"  P {row[frac]['P']:.3f}  R {row[frac]['R']:.3f}  F1 {row[frac]['F1']:.3f} "
                      f"[{row[frac]['F1_q25']:.3f}, {row[frac]['F1_q75']:.3f}]", flush=True)
            out[f"{system}-{names}-t{thr}"] = row
    (HERE / f"nbattract-{'' if INPUT == 'onto' else INPUT + '-'}{only or 'all'}.json").write_text(json.dumps(out, indent=1) + "\n")


if __name__ == "__main__":
    main()
