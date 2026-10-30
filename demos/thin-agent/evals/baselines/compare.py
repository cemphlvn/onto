"""The NBAttract comparison rule of EVALS.md (fixed before the native-input
results): per system and names condition, onto at zero labels against
NBAttract at k% labelled; win above NBAttract's 75th percentile, loss
below its 25th; label equivalent; a general claim only with 12 of 14
wins at a k.

    python3 compare.py
"""

import json
import math
from pathlib import Path

HERE = Path(__file__).parent
SYSTEMS = ["Teammates", "Argouml", "Prom", "Lucene", "Commonsimaging", "Ant", "Sweethome3d"]
BP = {"Argouml": "cookbook"}
KS = ["0.01", "0.05", "0.1", "0.2"]
JUDGES = {"Jev": "", "Gemini": "-j-openrouter_google_gemini_3.1_pro_preview"}


def nb(input_):
    """The corrected re-implementation (per-class counts, size priors) when
    its files exist, else the first runs (union documents)."""
    corrected, first = {"onto": ("nbattract-counts-*.json", "nbattract-[A-Z]*.json"),
                        "native": ("nbattract-native-counts-*.json", "nbattract-native-[A-Z]*.json")}[input_]
    files = list(HERE.glob(corrected)) or list(HERE.glob(first))
    out = {}
    for f in files:
        out.update(json.loads(f.read_text()))
    return out


def onto(system, cond, suffix):
    f = HERE.parent / "sysmdl" / f"l2-{system}-{cond}-bp-{BP.get(system, 'doc')}{suffix}.json"
    return json.loads(f.read_text())["discover"]["F1"] if f.exists() else None


def sign_p(wins, n):
    """Two-sided sign test, ties dropped beforehand."""
    if n == 0:
        return 1.0
    k = min(wins, n - wins)
    return min(1.0, 2 * sum(math.comb(n, i) for i in range(k + 1)) / 2**n)


def main():
    for input_ in ("onto", "native"):
        data = nb(input_)
        print(f"\n######## NBAttract on {'onto' if input_ == 'onto' else 'its native'} input (0.9 rule) ########")
        for judge, suffix in JUDGES.items():
            print(f"\n-- onto + {judge} at 0 labels vs NBAttract at k% labelled: F1 onto | NB median [q25, q75] -> W/T/L")
            print(f"{'system':15} {'names':8} {'onto':>5} | " + " | ".join(f"{float(k):>4.0%} NB" + " " * 14 for k in KS) + " | label equiv.")
            tally = {k: [0, 0, 0] for k in KS}
            for s in SYSTEMS:
                for cond in ("visible", "hidden"):
                    o = onto(s, cond, suffix)
                    row = data.get(f"{s}-{cond}-t0.9")
                    if o is None or row is None:
                        continue
                    cells, equiv = [], "> 20%"
                    for k in KS:
                        r = row[k]
                        verdict = "W" if o > r["F1_q75"] else ("L" if o < r["F1_q25"] else "T")
                        tally[k]["WTL".index(verdict)] += 1
                        cells.append(f"{r['F1']:.2f} [{r['F1_q25']:.2f},{r['F1_q75']:.2f}] {verdict}")
                        if equiv == "> 20%" and r["F1"] >= o:
                            equiv = f"{float(k):.0%}"
                    print(f"{s:15} {cond:8} {o:5.2f} | " + " | ".join(cells) + f" | {equiv}")
            for k in KS:
                w, t, l = tally[k]
                n = w + t + l
                claim = "general claim allowed" if n == 14 and w >= 12 else "no general claim"
                print(f"  at {float(k):.0%}: {w} win, {t} tie, {l} loss of {n}; sign test (ties dropped) p {sign_p(w, w + l):.3f}; {claim}")


if __name__ == "__main__":
    main()
