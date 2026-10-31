"""One table: the same model with onto and without it, beside Jev and the
cascade, on the decisions every arm has. Accuracy (all / names visible /
names hidden), failures, exact McNemar against Gemini-in-onto, tokens and
requests per decision, USD per 1000 decisions, and the wall-clock columns
timing.py measured.

    python3 onto_vs_direct.py [MODEL]    # default google/gemini-3.1-pro-preview

Cost: the direct arm's records carry OpenRouter's billed cost; the model's
per-token prices are fitted from them (least squares on input and output
tokens) and applied to onto's token counts, so both arms are priced the
same way. Jev's price is unknown: its tokens count, its cost does not.
"""

import json
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent / "calibration"))
from leaderboard import RECORDS, SEALED, mcnemar  # noqa: E402

HERE = Path(__file__).parent


def slug(s):
    return re.sub(r"[^A-Za-z0-9.]+", "_", s)


def arm_files(suffix):
    """(system, cond) -> records path, for a file-name suffix ('' = Jev)."""
    out = {}
    for p in RECORDS.glob("*.records.jsonl"):
        stem = p.name.removesuffix(".records.jsonl")
        m = re.match(r"(JabRef)-meaning-noroles-framing-arch(?:-judge-(.+))?$", stem) or \
            re.match(r"([A-Za-z0-9]+)-(visible|hidden)-bp-[a-z]+(?:-(.+))?$", stem)
        if not m:
            continue
        if m.group(1) == "JabRef":
            system, cond, rest = "JabRef", "visible", m.group(2) and f"j-{m.group(2)}"
        else:
            system, cond, rest = m.group(1), m.group(2), m.group(3)
        if (rest or "") == suffix:
            out[(system, cond)] = p
    return out


def load(suffix):
    """(system, cond, object) -> record dict with 'right' added."""
    out = {}
    for (system, cond), p in arm_files(suffix).items():
        gold = p.with_name(p.name.replace(".records.", ".gold."))
        if not gold.exists():
            continue
        truth = {json.loads(l)["object"]: json.loads(l)["target"] for l in gold.read_text().splitlines()}
        for line in p.read_text().splitlines():
            r = json.loads(line)
            if r["object"] in truth:
                r["right"] = r["disposition"] == "adopted" and r["target"] == truth[r["object"]]
                out[(system, cond, r["object"])] = r
    return out


def fit_prices(recs):
    """USD per token (input, output) from billed records: 2x2 normal equations."""
    xs = [(r["tokens"]["input"], r["tokens"]["output"], r["tokens"]["cost"]) for r in recs
          if (r.get("tokens") or {}).get("cost")]
    a11 = sum(i * i for i, o, c in xs); a12 = sum(i * o for i, o, c in xs); a22 = sum(o * o for i, o, c in xs)
    b1 = sum(i * c for i, o, c in xs); b2 = sum(o * c for i, o, c in xs)
    det = a11 * a22 - a12 * a12
    return (b1 * a22 - b2 * a12) / det, (a11 * b2 - a12 * b1) / det


def main():
    model = sys.argv[1] if len(sys.argv) > 1 else "google/gemini-3.1-pro-preview"
    judge = f"openrouter:{model}"
    arms = [  # label, file suffix
        (f"{model.split('/')[-1]} in onto", f"j-{slug(judge)}"),
        (f"{model.split('/')[-1]} in onto, brief", f"j-{slug(judge)}-brief"),
        (f"{model.split('/')[-1]} direct", f"j-{slug('direct:' + model)}"),
        ("Jev in onto", ""),
        ("cascade Jev → Gemini (p<0.7)", f"casc-{slug(judge)}-0.7"),
    ]
    data = {label: load(sfx) for label, sfx in arms}
    data = {k: v for k, v in data.items() if v}
    # 402s (an account out of credit) are not the arm's failures: those
    # decisions are left out until they are re-asked.
    unpaid = {k for v in data.values() for k, r in v.items() if "402" in str(r.get("error", ""))}
    common = set.intersection(*(set(v) for v in data.values())) - unpaid
    direct = next((v for k, v in data.items() if k.endswith(" direct")), {})
    p_in, p_out = fit_prices(list(direct.values())) if direct else (None, None)
    timing_file = HERE / "results" / f"timing-{slug(model)}.json"
    timing = {(r["arm"], r["concurrency"]): r for r in json.loads(timing_file.read_text())} \
        if timing_file.exists() else {}

    print(f"{len(common)} decisions shared by {len(data)} arms "
          f"({sum(k[1] == 'visible' for k in common)} names visible, {sum(k[1] == 'hidden' for k in common)} hidden; "
          f"{sum(k[0] in SEALED for k in common)} on sealed systems)")
    if p_in:
        print(f"fitted {model} price: ${p_in * 1e6:.2f} / M input, ${p_out * 1e6:.2f} / M output\n")
    base = data[arms[0][0]]
    head = f"{'arm':32} {'acc':>6} {'vis':>6} {'hid':>6} {'fail':>5} {'McNemar p':>10} {'in tok':>7} {'out tok':>7} {'req':>4} {'$/1k':>7}"
    print(head)
    print("-" * len(head))
    rows = []
    for label, sfx in arms:
        if label not in data:
            continue
        d = data[label]
        acc = lambda keys: sum(d[k]["right"] for k in keys) / len(keys) if keys else float("nan")
        vis = [k for k in common if k[1] == "visible"]
        hid = [k for k in common if k[1] == "hidden"]
        fail = sum(d[k]["disposition"] == "failed" for k in common)
        ab = {k: (d[k]["right"],) for k in common}
        p, b01, b10 = mcnemar(ab, {k: (base[k]["right"],) for k in common}, common) if d is not base else (None, 0, 0)
        tin = sum((d[k].get("tokens") or {}).get("input", 0) for k in common) / len(common)
        tout = sum((d[k].get("tokens") or {}).get("output", 0) for k in common) / len(common)
        req = sum((d[k].get("tokens") or {}).get("requests", 1) for k in common) / len(common)
        # Priced tokens: only the model's own answers (Jev's price is unknown).
        priced = [d[k] for k in common if model in d[k].get("judge", "")]
        cost = (sum((r.get("tokens") or {}).get("input", 0) * p_in + (r.get("tokens") or {}).get("output", 0) * p_out
                    for r in priced)
                / len(common) * 1000) if p_in and priced else None
        rows.append(dict(arm=label, accuracy=acc(list(common)), visible=acc(vis), hidden=acc(hid), failed=fail,
                         mcnemar_p=p, onto_only_right=b10, arm_only_right=b01, input_tokens=tin,
                         output_tokens=tout, requests=req, usd_per_1k=cost))
        print(f"{label:32} {acc(list(common)):6.3f} {acc(vis):6.3f} {acc(hid):6.3f} {fail:5d} "
              f"{'—' if p is None else f'{p:.2g} ({b01}/{b10})':>10} {tin:7.0f} {tout:7.0f} {req:4.1f} "
              f"{'—' if cost is None else f'{cost:7.2f}':>7}")
    if timing:
        print(f"\nwall clock on the timing slice (Lucene + TEAMMATES, names visible)")
        print(f"{'arm':10} {'in flight':>9} {'decisions':>9} {'seconds':>8} {'s/decision':>10} {'acc':>6} {'p50 call':>9} {'p90 call':>9}")
        for (arm, conc), r in sorted(timing.items()):
            print(f"{arm:10} {conc:9d} {r['decisions']:9d} {r['seconds']:8.1f} {r['seconds_per_decision']:10.2f} "
                  f"{r['accuracy']:6.3f} {r['latency_p50'] or '—':>9} {r['latency_p90'] or '—':>9}")
    (HERE / "results" / f"onto-vs-direct-{slug(model)}.json").write_text(
        json.dumps({"decisions": len(common), "price_per_token": [p_in, p_out], "arms": rows,
                    "timing": list(timing.values())}, indent=1) + "\n")


if __name__ == "__main__":
    main()
