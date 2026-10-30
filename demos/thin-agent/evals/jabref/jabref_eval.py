"""JabRef 3.7 (SAEroCon): our reader + the expert mapping + the rules,
graded against HUSACCT's labelled violations.

Unit of grading: (from class, to class or library) pairs that break a
rule, top-level classes only (inner classes folded into their owner).
"""

import json
import re
import sys
from collections import defaultdict
from pathlib import Path

import xlrd

HERE = Path(__file__).parent
REPO = HERE / "data/repo/systems/JabRef"
SRC = REPO / "sources/3.7/src/main/java"
ARCH = REPO / "architectures/JabRef-3.7-0001"

LAYERS = ["GUI", "Logic", "Model"]  # top to bottom
LIBS = {"java.awt": "Swing-AWT", "javax.swing": "Swing-AWT",
        "java.sql": "JavaSqlAndOracle", "oracle": "JavaSqlAndOracle"}


def top_level(fqn):
    parts = fqn.split(".")
    for i, p in enumerate(parts):
        if p[:1].isupper():
            return ".".join(parts[: i + 1])
    return fqn


def library(fqn):
    for prefix, lib in LIBS.items():
        if fqn == prefix or fqn.startswith(prefix + "."):
            return lib
    return None


def load_mapping():
    book = xlrd.open_workbook(ARCH / "JabRef 3.7 Intended Architecture - Final.xls")
    sheet = book.sheet_by_name("All Modules")
    units, module = [], None
    for r in range(4, sheet.nrows):
        row = [str(c.value) for c in sheet.row(r)]
        if row[2]:
            module = row[2].split(" (")[0].strip()
        if row[3] and "(library)" not in row[3]:
            name, kind = row[3].rsplit(" (", 1)
            units.append((name.strip(), kind.rstrip(")"), module))
    return units


def module_of(cls, units):
    best = None
    for name, kind, module in units:
        if kind == "package":
            if cls.startswith(name + ".") and (best is None or len(name) > len(best[0])):
                best = (name, module)
        elif cls == name:
            return module
    return best[1] if best else None


def load_rules():
    book = xlrd.open_workbook(ARCH / "JabRef 3.7 Intended Architecture - Final.xls")
    sheet = book.sheet_by_name("All Rules with Exceptions")
    forbidden = {}  # (from module, to module or library) -> rule text
    only = []
    for r in range(4, sheet.nrows):
        _, _, _, frm, kind, to, _ = [str(c.value) for c in sheet.row(r)]
        if kind == "Is not allowed to use":
            forbidden[(frm, to)] = f"{frm} is not allowed to use {to}"
        elif kind == "Is the only module allowed to use":
            only.append((frm, to.split(".", 1)[1]))
        elif kind == "Is not allowed to back call":
            i = LAYERS.index(frm)
            for upper in LAYERS[:i]:
                forbidden[(frm, upper)] = f"{frm} is not allowed to back call {upper}"
        elif kind == "Is not allowed to skip call":
            i = LAYERS.index(frm)
            for lower in LAYERS[i + 2 :]:
                forbidden[(frm, lower)] = f"{frm} is not allowed to skip call {lower}"
    modules = {m for _, _, m in load_mapping()} | {"CommandLineInterface"}
    for owner, lib in only:
        for m in modules - {owner}:
            forbidden[(m, lib)] = f"only {owner} may use {lib}"
    return forbidden


IMPORT = re.compile(r"^\s*import\s+(static\s+)?([\w.]+)(\.\*)?\s*;", re.M)
PACKAGE = re.compile(r"^\s*package\s+([\w.]+)\s*;", re.M)
STRIP = re.compile(r'//[^\n]*|/\*.*?\*/|"(?:\\.|[^"\\])*"|\'(?:\\.|[^\'\\])*\'', re.S)
QUALIFIED = re.compile(r"\b((?:[a-z]\w*\.)+[A-Z]\w*)")
SIMPLE = re.compile(r"\b([A-Z]\w*)\b")


def read_sources():
    """class FQN -> set of referenced FQNs (project classes or library
    packages), read from imports, qualified names and simple names
    resolved against imports and the class's own package."""
    files = sorted(SRC.rglob("*.java"))
    classes = {}
    for f in files:
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
                if library(name):
                    refs.add(name)
                elif name in classes:  # import pkg.Class.*
                    refs.add(name)
                continue
            target = top_level(name) if not static else top_level(name.rsplit(".", 1)[0])
            explicit[target.rsplit(".", 1)[-1]] = target
            refs.add(target if not library(target) else name)
        body = STRIP.sub(" ", IMPORT.sub(" ", PACKAGE.sub(" ", text)))
        for q in QUALIFIED.findall(body):
            if q in classes or library(q):
                refs.add(top_level(q) if q in classes else q)
        for simple in set(SIMPLE.findall(body)):
            if simple in explicit:
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


def truth():
    sheet = xlrd.open_workbook(ARCH / "JabRef 3.7 Violations - Final.xls").sheet_by_name("Violations_1")
    pairs = defaultdict(list)
    for r in range(1, sheet.nrows):
        frm, to, kind, sub, line, direct, *_ , rule = [str(c.value) for c in sheet.row(r)]
        to = to.removeprefix("xLibraries.")
        to_unit = library(to) or top_level(to)
        pairs[(top_level(frm), to_unit)].append((kind, direct, rule))
    return pairs


def main():
    units, forbidden = load_mapping(), load_rules()
    deps = read_sources()
    predicted = {}
    for frm, refs in deps.items():
        mf = module_of(frm, units)
        for to in refs:
            lib = library(to)
            mt = lib or module_of(to, units)
            if mf and mt and (mf, mt) in forbidden:
                predicted[(frm, lib or to)] = forbidden[(mf, mt)]
    gold = truth()
    tp = set(predicted) & set(gold)
    fp = set(predicted) - set(gold)
    fn = set(gold) - set(predicted)
    p = len(tp) / len(predicted) if predicted else 0
    r = len(tp) / len(gold) if gold else 0
    f1 = 2 * p * r / (p + r) if p + r else 0
    unmapped = sorted(c for c in deps if module_of(c, units) is None)
    print(f"classes read {len(deps)}, unmapped by the expert {len(unmapped)}")
    print(f"gold pairs {len(gold)} (from {sum(len(v) for v in gold.values())} rows), predicted {len(predicted)}")
    print(f"TP {len(tp)}  FP {len(fp)}  FN {len(fn)}   P {p:.3f}  R {r:.3f}  F1 {f1:.3f}")
    json.dump({
        "fp": sorted([*k, v] for k, v in predicted.items() if k in fp),
        "fn": sorted([*k, gold[k]] for k in fn),
        "tp": sorted(map(list, tp)),
    }, open(HERE / "result.json", "w"), indent=1)


if __name__ == "__main__":
    main()
