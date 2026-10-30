"""An LLM used directly, without onto: the same package descriptions and
module descriptions onto's discover shows its judge, sent straight to the
model through the OpenRouter API, one package at a time. No substrate:
no search, no gate, no records of onto's, no checks. Writes decision
records in the format `leaderboard.py` reads, under the judge name
`direct:<model>`, so the substrate's own contribution can be isolated.

    python3 direct_llm.py MODEL [--jobs=8]     # e.g. google/gemini-3.1-pro-preview

Reads the categories the onto runs generated (`../sysmdl/l2-*.onto`,
`../jabref/l2-*.onto`) and the gold files in `../calibration/records`.
"""

import json
import os
import re
import sys
import time
import urllib.request
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

HERE = Path(__file__).parent
EVALS = HERE.parent
RECORDS = EVALS / "calibration" / "records"
RUNS = [  # (onto category file, gold/record tag)
    *[(EVALS / "sysmdl" / f"l2-{s}-{c}-bp-{bp}.onto", f"{s}-{c}-bp-{bp}")
      for s, bp in [("Teammates", "doc"), ("Argouml", "cookbook"), ("Prom", "doc"), ("Lucene", "doc"),
                    ("Commonsimaging", "doc"), ("Ant", "doc"), ("Sweethome3d", "doc")]
      for c in ("visible", "hidden")],
    (EVALS / "jabref" / "l2-meaning-noroles-framing-arch.onto", "JabRef-meaning-noroles-framing-arch"),
]
LETTERS = "ABCDEFGHIJKLMNOPQRSTUVWXY"


def parse(path):
    """Code objects with their descriptions and dependencies; modules with
    theirs. Categories are written by the eval scripts, one per file."""
    text = path.read_text()
    code, intended = text.split("category Intended")
    about = lambda part: dict(re.findall(r'^\s*about (\w+): "(.*)";$', part, re.M))
    deps = {}
    for a, b in re.findall(r"^\s*d_\w+: (\w+) -> (\w+)", code, re.M):
        deps.setdefault(a, []).append(b)
    modules = re.search(r"objects: ([^;]+);", intended).group(1).split(", ")
    return about(code), deps, modules, about(intended)


def ask(model, prompt):
    body = json.dumps({
        "model": model, "temperature": 0, "response_format": {"type": "json_object"},
        "messages": [
            {"role": "system", "content": "You map source code to the modules of an intended software architecture. Reply with JSON only: an object mapping every option letter to your probability that it is the right one; the probabilities sum to 1."},
            {"role": "user", "content": prompt},
        ],
    }).encode()
    req = urllib.request.Request("https://openrouter.ai/api/v1/chat/completions", data=body, headers={
        "Authorization": f"Bearer {os.environ['OPENROUTER_API_KEY']}", "Content-Type": "application/json"})
    for attempt in range(6):
        try:
            with urllib.request.urlopen(req, timeout=120) as r:
                v = json.loads(r.read())
            text = v["choices"][0]["message"]["content"]
            probs = json.loads(text[text.find("{"): text.rfind("}") + 1])
            return probs, v.get("usage", {})
        except Exception as e:  # noqa: BLE001 - retried, then recorded as failed
            last = e
            time.sleep(min(30, 2 ** attempt))
    return None, {"error": str(last)}


def one(model, onto_file, tag):
    about, deps, modules, mabout = parse(onto_file)
    opts = "\n".join(f"{LETTERS[i]}. {m}: {mabout.get(m, m)}" for i, m in enumerate(modules)) + "\nZ. none of these"
    out = []
    for obj, desc in about.items():
        dep_text = ", ".join(about.get(d, d) for d in deps.get(obj, [])[:20]) or "none"
        prompt = (f"Code unit: {desc}\nIt depends on: {dep_text}\n\n"
                  f"Which module of the intended architecture is responsible for this code unit?\n{opts}")
        probs, usage = ask(model, prompt)
        if probs is None:
            out.append({"object": obj, "disposition": "failed", "target": None, "p": None, "error": usage.get("error")})
            continue
        ranked = sorted(((float(probs.get(LETTERS[i], 0) or 0), m) for i, m in enumerate(modules)), reverse=True)
        none = float(probs.get("Z", 0) or 0)
        p, target = ranked[0]
        out.append({"object": obj, "disposition": "adopted" if p >= none else "outside", "target": target, "p": p,
                    "none": none, "judge": f"direct:{model}",
                    "candidates": [{"target": m, "p": q} for q, m in ranked[:3]],
                    "tokens": {"input": usage.get("prompt_tokens", 0), "output": usage.get("completion_tokens", 0),
                               "cost": usage.get("cost")}})
    return out


def record_path(tag, model):
    slug = re.sub(r"[^A-Za-z0-9.]+", "_", f"direct:{model}")
    return RECORDS / (f"{tag}-judge-{slug}.records.jsonl" if tag.startswith("JabRef") else f"{tag}-j-{slug}.records.jsonl")


def main():
    model = sys.argv[1]
    jobs = next((int(a.split("=")[1]) for a in sys.argv[2:] if a.startswith("--jobs=")), 8)
    todo = [(f, t) for f, t in RUNS if f.exists() and not record_path(t, model).exists()]
    print(f"direct:{model}: {len(todo)} runs to do", flush=True)

    def work(ft):
        f, tag = ft
        recs = one(model, f, tag)
        path = record_path(tag, model)
        path.write_text("".join(json.dumps(r) + "\n" for r in recs))
        gold = RECORDS / f"{tag}.gold.jsonl"
        path.with_name(path.name.replace(".records.", ".gold.")).write_text(gold.read_text())
        failed = sum(r["disposition"] == "failed" for r in recs)
        return f"{tag:40} {len(recs)} decisions, {failed} failed"

    with ThreadPoolExecutor(max_workers=jobs) as pool:
        for line in pool.map(work, todo):
            print(line, flush=True)


if __name__ == "__main__":
    main()
