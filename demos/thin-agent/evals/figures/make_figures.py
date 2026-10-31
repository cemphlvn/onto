"""Figures for the thin-agent results (EVALS.md), from the committed records.

    uv run --with matplotlib --with seaborn --with pandas --with numpy python make_figures.py

Writes out/fig*.png and .pdf. Every figure reads records or result files;
a judge or run that is missing is left out, not faked.
"""

import glob
import json
import sys
from collections import defaultdict
from pathlib import Path

import matplotlib.pyplot as plt
import numpy as np
import pandas as pd
import seaborn as sns

HERE = Path(__file__).parent
EVALS = HERE.parent
OUT = HERE / "out"
OUT.mkdir(exist_ok=True)
sys.path.insert(0, str(EVALS / "calibration"))
import leaderboard as L  # noqa: E402

SYSTEMS = ["JabRef", "Teammates", "Argouml", "Prom", "Lucene", "Commonsimaging", "Ant", "Sweethome3d"]
LABEL = {"JabRef": "JabRef", "Teammates": "TEAMMATES", "Argouml": "ArgoUML", "Prom": "ProM", "Lucene": "Lucene",
         "Commonsimaging": "Commons Imaging", "Ant": "Ant", "Sweethome3d": "Sweet Home 3D"}
# One colour per judge, kept in every figure.
JUDGES = {
    "jev": ("Jev", "#1f4e79"),
    "openrouter_google_gemini_3.1_pro_preview": ("Gemini 3.1 Pro (onto)", "#c0504d"),
    "direct_google_gemini_3.1_pro_preview": ("Gemini 3.1 Pro (direct, no onto)", "#e8a09a"),
    "openrouter_qwen_qwen3.5_397b_a17b": ("Qwen 3.5 397B (onto)", "#4f9a5f"),
    "openrouter_x_ai_grok_4.3": ("Grok 4.3 (onto, partial)", "#8064a2"),
}
CASCADE = ("Cascade: Jev, Gemini below p 0.7", "#d08a2c")
NB = "#7f7f7f"

sns.set_theme(style="whitegrid", context="paper", font_scale=1.05)
plt.rcParams.update({"axes.spines.top": False, "axes.spines.right": False, "savefig.dpi": 200,
                     "savefig.bbox": "tight", "legend.frameon": False, "grid.alpha": 0.35})


def save(fig, name):
    for ext in ("png", "pdf"):
        fig.savefig(OUT / f"{name}.{ext}")
    plt.close(fig)
    print("wrote", name)


def judge_frame():
    rows = []
    for slug, (name, _) in JUDGES.items():
        for (system, cond, obj), (right, p, tin, tout) in L.load(slug).items():
            rows.append(dict(judge=slug, name=name, system=system, cond=cond, obj=obj, right=right, p=p))
    return pd.DataFrame(rows)


def cascade_frame():
    rows = []
    for f in glob.glob(str(L.RECORDS / "*-casc-*.records.jsonl")):
        tag = Path(f).name.split("-casc-")[0]
        system, cond = tag.split("-")[:2]
        gold = {json.loads(l)["object"]: json.loads(l)["target"] for l in (L.RECORDS / f"{tag}.gold.jsonl").read_text().splitlines()}
        for line in Path(f).read_text().splitlines():
            r = json.loads(line)
            if r["object"] in gold:
                rows.append(dict(system=system, cond=cond, obj=r["object"], p=r["p"] or 0.0,
                                 right=r["disposition"] == "adopted" and r["target"] == gold[r["object"]],
                                 escalated="gemini" in (r.get("judge") or "")))
    return pd.DataFrame(rows)


# Figure 1: judges behind the same contract, accuracy per system.
def fig_judges(df):
    acc = df.groupby(["name", "judge", "system", "cond"])["right"].mean().reset_index()
    fig, axes = plt.subplots(1, 2, figsize=(10, 4.2), sharey=True)
    order = [s for s in SYSTEMS if s in set(acc.system)]
    for ax, cond in zip(axes, ["visible", "hidden"]):
        sub = acc[acc.cond == cond]
        for k, (slug, (name, colour)) in enumerate(JUDGES.items()):
            d = sub[sub.judge == slug].set_index("system").reindex(order)
            y = np.arange(len(order)) + (k - 2) * 0.12
            ax.scatter(d["right"], y, s=34, color=colour, label=name, zorder=3, edgecolor="white", linewidth=0.6)
        ax.set_yticks(np.arange(len(order)), [LABEL[s] for s in order])
        ax.set_xlim(0.3, 1.02)
        ax.set_xlabel("accuracy (package decisions)")
        ax.set_title(f"package names {cond}", loc="left", fontsize=11)
        ax.invert_yaxis()
    axes[0].legend(loc="lower left", bbox_to_anchor=(0, 1.08), ncol=3, fontsize=8.5)
    fig.suptitle("Judges swapped behind onto's Judge contract, same frames and blueprints", x=0.01, ha="left", y=1.12, fontsize=12)
    save(fig, "fig1_judges")


# Figure 2: NBAttract learning curves against onto at zero labels.
def fig_nbattract():
    def load(pattern):
        out = {}
        for f in (EVALS / "baselines").glob(pattern):
            out.update(json.loads(f.read_text()))
        return out
    native, same = load("nbattract-native-counts-*.json"), load("nbattract-counts-*.json")
    if not native:
        return

    def onto_f1(system, cond, suffix=""):
        bp = "cookbook" if system == "Argouml" else "doc"
        f = EVALS / "sysmdl" / f"l2-{system}-{cond}-bp-{bp}{suffix}.json"
        return json.loads(f.read_text())["discover"]["F1"] if f.exists() else None

    systems = [s for s in SYSTEMS if f"{s}-hidden-t0.9" in native]
    fig, axes = plt.subplots(2, len(systems), figsize=(2.15 * len(systems), 4.8), sharey=True, sharex=True)
    ks = ["0.0", "0.01", "0.05", "0.1", "0.2"]
    x = [0, 1, 2, 3, 4]  # evenly spaced: 0, 1, 5, 10, 20% labelled
    for col, s in enumerate(systems):
        for row, cond in enumerate(["visible", "hidden"]):
            ax = axes[row, col]
            r = native[f"{s}-{cond}-t0.9"]
            med = [r[k]["F1"] for k in ks]
            ax.fill_between(x, [r[k]["F1_q25"] for k in ks], [r[k]["F1_q75"] for k in ks], color=NB, alpha=0.18, lw=0)
            ax.plot(x, med, color=NB, lw=1.8, marker="o", ms=3.5, label="NBAttract, native input (median, IQR)")
            if f"{s}-{cond}-t0.9" in same:
                ax.plot(x, [same[f"{s}-{cond}-t0.9"][k]["F1"] for k in ks], color=NB, lw=1.2, ls=":",
                        label="NBAttract, onto's input")
            for suffix, slug, ls in (("", "jev", "-"), ("-j-openrouter_google_gemini_3.1_pro_preview",
                                                         "openrouter_google_gemini_3.1_pro_preview", "--")):
                v = onto_f1(s, cond, suffix)
                if v is not None:
                    ax.axhline(v, color=JUDGES[slug][1], lw=1.6, ls=ls, label=f"onto + {JUDGES[slug][0].split(' (')[0]}, 0 labels")
            ax.set_ylim(0, 1.03)
            ax.set_xticks(x, ["0", "1", "5", "10", "20"])
            if row == 0:
                ax.set_title(LABEL[s], fontsize=10)
            if col == 0:
                ax.set_ylabel(f"F1, names {cond}")
            if row == 1:
                ax.set_xlabel("% classes labelled")
    handles, labels = axes[0, 0].get_legend_handles_labels()
    fig.legend(handles, labels, loc="lower left", bbox_to_anchor=(0.01, 0.98), ncol=4, fontsize=8.5)
    fig.suptitle("Zero-label onto against NBAttract trained on labelled classes", x=0.01, ha="left", y=1.07, fontsize=12)
    fig.tight_layout()
    save(fig, "fig2_nbattract")


# Figure 3: routing by reliability: accuracy against cost and against people asked.
def fig_routing(df, casc):
    jev = df[df.judge == "jev"].set_index(["system", "cond", "obj"])
    gem = df[df.judge == "openrouter_google_gemini_3.1_pro_preview"].set_index(["system", "cond", "obj"])
    keys = jev.index.intersection(gem.index)
    keys = keys[[k[0] != "JabRef" for k in keys]]
    j, g = jev.loc[keys], gem.loc[keys]
    usd_per_gemini_call = 25.6 / 1000  # measured mean per decision (EVALS Result 12)
    fig, (a1, a2) = plt.subplots(1, 2, figsize=(10, 4))
    ts = np.linspace(0, 1, 41)
    acc = [np.where(j.p < t, g.right, j.right).mean() for t in ts]
    share = [(j.p < t).mean() for t in ts]
    a1.plot(np.array(share) * usd_per_gemini_call * 1000, acc, color=CASCADE[1], lw=2, label="cascade, threshold swept (from records)")
    a1.scatter([0], [j.right.mean()], color=JUDGES["jev"][1], s=50, zorder=3, label="Jev alone")
    a1.scatter([usd_per_gemini_call * 1000], [g.right.mean()], color=JUDGES["openrouter_google_gemini_3.1_pro_preview"][1], s=50, zorder=3, label="Gemini alone")
    if len(casc):
        real = casc[casc.system != "JabRef"]
        a1.scatter([real.escalated.mean() * usd_per_gemini_call * 1000], [real.right.mean()], marker="*", s=160,
                   color=CASCADE[1], edgecolor="black", linewidth=0.6, zorder=4, label="cascade at 0.7, run in onto")
    a1.set_xlabel("Gemini cost, USD per 1,000 decisions")
    a1.set_ylabel("accuracy")
    a1.set_title("Second judge only where the first is unsure", loc="left", fontsize=11)
    a1.legend(fontsize=8.5, loc="lower right")
    # People asked: Jev -> person, and Jev -> Gemini -> person.
    for label, colour, f in (
        ("Jev, a person below t", JUDGES["jev"][1],
         lambda t: (np.where(j.p < t, True, j.right).mean(), (j.p < t).mean())),
        ("Jev, Gemini below 0.9, a person below t", CASCADE[1],
         lambda t: (np.where(j.p < 0.9, np.where(g.p < t, True, g.right), j.right).mean(), ((j.p < 0.9) & (g.p < t)).mean())),
    ):
        pts = np.array([f(t) for t in ts])
        a2.plot(pts[:, 1] * 100, pts[:, 0], color=colour, lw=2, label=label)
    a2.set_xlabel("% decisions sent to a person")
    a2.set_ylabel("accuracy (person simulated by the expert mapping)")
    a2.set_title("A person sees fewer decisions with a second judge", loc="left", fontsize=11)
    a2.legend(fontsize=8.5, loc="lower right")
    fig.tight_layout()
    save(fig, "fig3_routing")


# Figure 4: reliability diagrams per judge.
def fig_reliability(df):
    """Reliability (top) and sharpness (bottom): a judge whose stated p sits
    only at 0 and 1 can look calibrated and still give a gate nothing to
    route on."""
    shown = ["jev", "openrouter_google_gemini_3.1_pro_preview", "openrouter_qwen_qwen3.5_397b_a17b",
             "direct_google_gemini_3.1_pro_preview"]
    shown = [s for s in shown if s in set(df.judge)]
    fig, axes = plt.subplots(2, len(shown), figsize=(3.2 * len(shown), 4.6), sharex=True,
                             gridspec_kw={"height_ratios": [3, 1]})
    bins = np.linspace(0, 1, 11)
    for col, slug in enumerate(shown):
        d = df[df.judge == slug]
        colour = JUDGES[slug][1]
        idx = np.clip(np.digitize(d.p, bins) - 1, 0, 9)
        stats = [(d.p[idx == b].mean(), d.right[idx == b].mean(), (idx == b).sum()) for b in range(10) if (idx == b).any()]
        ece = sum(n / len(d) * abs(a - p) for p, a, n in stats)
        ax = axes[0, col]
        ax.plot([0, 1], [0, 1], color="#bbbbbb", lw=1, ls="--")
        ax.scatter([p for p, _, _ in stats], [a for _, a, _ in stats], s=[18 + 140 * n / len(d) for _, _, n in stats],
                   color=colour, edgecolor="white", linewidth=0.6, zorder=3)
        ax.set_ylim(-0.03, 1.03)
        ax.set_title(f"{JUDGES[slug][0]}\nECE {ece:.3f}", fontsize=9.5, loc="left")
        hist = axes[1, col]
        hist.hist(d.p, bins=bins, color=colour, alpha=0.85, rwidth=0.9)
        hist.set_yscale("log")
        hist.yaxis.set_minor_formatter(plt.NullFormatter())
        hist.set_xlabel("stated probability")
        hist.set_xlim(0, 1)
    axes[0, 0].set_ylabel("share right")
    axes[1, 0].set_ylabel("decisions (log)")
    fig.suptitle("Calibration (top, point size = decisions) and sharpness (bottom) of each judge's probability",
                 x=0.01, ha="left", y=1.04, fontsize=12)
    fig.tight_layout()
    save(fig, "fig4_reliability")


# Figure 5: ablation of the thin-agent parts (JabRef, class level).
def fig_ablation():
    def f1(tag):
        p = EVALS / "jabref" / "results" / f"l2-{tag}.json"
        return json.loads(p.read_text())["variants"]["discover"]["F1"] if p.exists() else None
    full = f1("structure-noroles-framing-arch")
    rows = [("substrate", "role pruning back on", "structure-framing-arch"),
            ("substrate", "path constraint removed", "meaning-noroles-framing-arch"),
            ("blueprint", "default framing", "structure-noroles"),
            ("blueprint", "module names only", "structure-noroles-framing-arch-target-names"),
            ("agent input", "package name only", "structure-noroles-framing-arch-source-package")]
    data = [(role, what, f1(tag) - full) for role, what, tag in rows if f1(tag) is not None]
    if full is None or not data:
        return
    fig, ax = plt.subplots(figsize=(6.2, 2.8))
    colours = {"substrate": "#1f4e79", "blueprint": "#d08a2c", "agent input": "#7f7f7f"}
    for i, (role, what, d) in enumerate(data):
        ax.barh(i, d, color=colours[role], height=0.55)
        ax.text(d + (0.004 if d >= 0 else -0.004), i, f"{d:+.3f}", va="center", ha="left" if d >= 0 else "right", fontsize=8.5)
    ax.set_yticks(range(len(data)), [f"{what}  ({role})" for role, what, _ in data])
    ax.axvline(0, color="black", lw=0.8)
    lo = min(d for _, _, d in data)
    ax.set_xlim(lo - 0.035, max(0.0, max(d for _, _, d in data)) + 0.03)
    ax.invert_yaxis()
    ax.set_xlabel(f"ΔF1 against the full configuration (F1 {full:.3f}), JabRef")
    ax.set_title("What each part is worth: one part removed at a time", loc="left", fontsize=11)
    save(fig, "fig5_ablation")


# Figure 6: the inference engine: hierarchical walk against flat discover.
def fig_walk():
    rows = [json.loads(Path(f).read_text()) for f in glob.glob(str(EVALS / "walk" / "walk-*.result.json"))]
    if not rows:
        return
    fig, ax = plt.subplots(figsize=(5.2, 3.6))
    for r in rows:
        colour = "#c0504d" if r["walk_accuracy"] < r["flat_accuracy"] - 0.05 else (
            "#4f9a5f" if r["walk_accuracy"] > r["flat_accuracy"] + 0.05 else "#7f7f7f")
        ax.plot([0, 1], [r["flat_accuracy"], r["walk_accuracy"]], color=colour, lw=2, marker="o")
        ax.text(1.04, r["walk_accuracy"], f"{LABEL[r['system']]}, names {r['names']}", va="center", fontsize=8.5)
    ax.set_xticks([0, 1], ["flat discover\n(one choice)", "hierarchical walk\n(group, then module)"])
    ax.set_xlim(-0.15, 1.9)
    ax.set_ylabel("accuracy (class level)")
    ax.set_title("A hierarchy helps only where the document's split is the expert's", loc="left", fontsize=11)
    save(fig, "fig6_walk")


def main():
    df = judge_frame()
    casc = cascade_frame()
    fig_judges(df)
    fig_nbattract()
    fig_routing(df, casc)
    fig_reliability(df)
    fig_ablation()
    fig_walk()


if __name__ == "__main__":
    main()
