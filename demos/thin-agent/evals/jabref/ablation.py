"""Ablation table: each part of the thin-agent pipeline removed from the
full configuration F, one at a time; deltas against F (mean of its
repeats). Post-hoc parts (doc rules, threshold, Platt) are read off F's
variants."""

import json
from pathlib import Path

HERE = Path(__file__).parent
F = ["structure-noroles-framing-arch", "structure-noroles-framing-arch-rep-2"]
ROWS = [
    ("substrate", "path constraint (allowed module dependencies)", "meaning-noroles-framing-arch"),
    ("substrate", "role pruning switched back on", "structure-framing-arch"),
    ("blueprint", "framing: architecture -> default (merger)", "structure-noroles"),
    ("blueprint", "target descriptions: expert doc -> names only", "structure-noroles-framing-arch-target-names"),
    ("agent input", "source descriptions: classes -> package name only", "structure-noroles-framing-arch-source-package"),
]
KEYS = ("coverage", "P", "R", "F1", "macro_F1")


def load(tag):
    path = HERE / f"l2-{tag}.json"
    return json.loads(path.read_text()) if path.exists() else None


def main():
    full = [load(t) for t in F]
    full = [f for f in full if f]
    base = {k: sum(f["variants"]["discover"][k] for f in full) / len(full) for k in KEYS}
    noise = {k: abs(full[0]["variants"]["discover"][k] - full[-1]["variants"]["discover"][k]) for k in KEYS}
    print(f"F (n={len(full)}): " + "  ".join(f"{k} {base[k]:.3f}" for k in KEYS))
    print("repeat spread:      " + "  ".join(f"{k} {noise[k]:.3f}" for k in KEYS))
    print()
    print(f"{'role':12} {'removed':52} " + " ".join(f"{'Δ' + k:>9}" for k in KEYS))
    for role, what, tag in ROWS:
        r = load(tag)
        if not r:
            print(f"{role:12} {what:52} (missing)")
            continue
        v = r["variants"]["discover"]
        print(f"{role:12} {what:52} " + " ".join(f"{v[k] - base[k]:+9.3f}" for k in KEYS))
    print()
    f0 = full[0]
    print("post-hoc on F (same run, no new judge calls):")
    for name, v in f0["variants"].items():
        print(f"  {name:62} " + "  ".join(f"{k} {v[k]:.3f}" for k in KEYS))
    cal = f0.get("calibration")
    if cal:
        print(f"  calibration raw   Brier {cal['raw']['brier']}  ECE {cal['raw']['ece_5bins']}")
        print(f"  calibration Platt Brier {cal['platt_loo']['brier']}  ECE {cal['platt_loo']['ece_5bins']}")


if __name__ == "__main__":
    main()
