#!/usr/bin/env python3
"""Each rule must bite: break the flow one way at a time and check that
onto refuses it, naming the broken invariant. Exit 1 if any mutant is
accepted or refused for another reason.

    python3 counterexamples.py
"""

import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).parent
ONTO = HERE.parents[2] / "target/release/onto"
FLOW = (HERE / "thinagent-ci.flow").read_text()

# (rule, what the mutant does, old line, new line, the invariant(s) onto may
# name). Tuning the blueprint on the verdict breaks two rules: it also lets
# the standard shape the proposal (standard -> verdict -> blueprint ->
# proposal), and onto reports the first violation it finds.
MUTANTS = [
    ("MapBeforeCheck", "Discover reads the standard (checks while mapping)",
     "blueprint, deps -> thinagent::Discover@0 -> proposal",
     "blueprint, deps, standard -> thinagent::Discover@0 -> proposal",
     "never: standard -> proposal"),
    ("GateInSubstrate", "the proposal is adopted without calibration or gate",
     "accepted, answers -> thinagent::Adopt@0 -> roles",
     "proposal, accepted, answers -> thinagent::Adopt@0 -> roles",
     "proposal -> roles through scored"),
    ("NeverGuess", "deferred mappings are adopted without a person",
     "accepted, answers -> thinagent::Adopt@0 -> roles",
     "accepted, deferred, answers -> thinagent::Adopt@0 -> roles",
     "deferred -> roles through answers"),
    ("BlueprintFrozenBeforeTest", "the verdict tunes the blueprint",
     "verdict -> thinagent::Record@0 -> record",
     "verdict -> thinagent::Record@0 -> record\nverdict -> thinagent::Tune@0 -> blueprint",
     ("never: verdict -> blueprint", "never: standard -> proposal")),
    ("CI", "the merge decision is taken from the roles, skipping the check",
     "verdict -> thinagent::CIGate@0 -> decision",
     "roles -> thinagent::CIGate@0 -> decision",
     "proposal -> decision through verdict"),
]


def check(flow_text, tmp):
    flow = tmp / "mutant.flow"
    flow.write_text(flow_text)
    gen = subprocess.run([sys.executable, HERE / "flow2onto.py", flow, HERE / "rules.onto-part", HERE / "roles.json"],
                         capture_output=True, text=True, check=True).stdout
    # imports resolve relative to the file: keep the generated file here.
    out = HERE / ".mutant.onto"
    out.write_text(gen)
    try:
        r = subprocess.run([ONTO, "check", out], capture_output=True, text=True)
    finally:
        out.unlink()
    return r.returncode, r.stdout + r.stderr


def main():
    bad = 0
    with tempfile.TemporaryDirectory() as t:
        code, text = check(FLOW, Path(t))
        print(f"{'original flow':28} {'accepted' if code == 0 else 'REFUSED'}")
        bad += code != 0
        for rule, what, old, new, expect in MUTANTS:
            assert old in FLOW, old
            code, text = check(FLOW.replace(old, new), Path(t))
            named = any(e in text for e in ((expect,) if isinstance(expect, str) else expect))
            ok = code != 0 and named
            bad += not ok
            why = next((l for l in text.splitlines() if "invariant" in l), "").split("is violated")[0].replace("error: ", "")
            print(f"{rule:28} {'refused' if code else 'ACCEPTED'}{'' if named or not code else ' (other reason)'}  — {what}; {why.strip()}")
            if not ok:
                print("   ", text.strip()[:300])
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
