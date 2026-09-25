#!/usr/bin/env python3
"""E3 cases: a world state first, then three channels rendered from it.

Truth is the situation class that generated the case, by construction.
Each channel (message, ledger, events) independently shows the truth
(clear), nothing unusual (absent), or a confusable neighbour situation
(misleading), per data/design.json. The ledger and the event log are
rendered from templates here; customer messages are drawn from pools the
generator model wrote per (situation, mode), stored in data/messages/.

python3 generate.py pools    # OPENROUTER_API_KEY: writes data/messages/
python3 generate.py cases    # deterministic: writes data/cases.<condition>.jobs + data/truth.json
"""
import json
import os
import random
import sys
import urllib.request
from pathlib import Path

HERE = Path(__file__).resolve().parent
D = json.loads((HERE / "data/design.json").read_text())
CLASSES = [c["id"] for c in D["classes"]]
DESC = {c["id"]: c["describe"] for c in D["classes"]}
NEIGHBOUR = D["neighbour"]


# ---- ledger: orders, payments, account record --------------------------------
def ledger(kind, rng):
    order = f"ORD-{rng.randint(10000, 99999)}"
    amount = rng.choice([12, 29, 36, 48, 63, 74, 120])
    base = {"account": {"status": "active", "email_changed": None, "recovery_changed": None},
            "orders": [{"id": order, "amount": amount, "status": "delivered", "sku_ordered": "SKU-A1",
                        "sku_shipped": "SKU-A1", "claims": []}],
            "payments": [{"order": order, "amount": amount, "status": "settled"}]}
    o, p, a = base["orders"][0], base["payments"], base["account"]
    if kind == "double_charge":
        p.append({"order": order, "amount": amount, "status": "settled"})
    elif kind == "refund_request":
        o["status"] = "cancelled by customer within trial"
        o["refund"] = "not issued"
    elif kind == "payment_failed":
        p[0]["status"] = "declined (card expired)"
        o["status"] = "awaiting payment"
    elif kind == "not_delivered":
        o["status"] = f"in transit, last carrier scan {rng.randint(8, 14)} days ago, expected {rng.randint(3, 5)} days after dispatch"
    elif kind == "damaged":
        o["claims"].append("claim opened: item arrived damaged")
    elif kind == "wrong_item":
        o["sku_shipped"] = "SKU-Z9"
    elif kind == "account_locked":
        a["status"] = "locked after 5 failed sign-in attempts"
    elif kind == "account_takeover":
        a["email_changed"] = "yesterday, from a new device abroad"
        a["recovery_changed"] = "yesterday, from a new device abroad"
    return base


# ---- event log -----------------------------------------------------------------
def events(kind, rng):
    t = lambda: f"2026-09-{rng.randint(1, 24):02d}T{rng.randint(0, 23):02d}:{rng.randint(0, 59):02d}Z"
    noise = [f"{t()} session.start device=known", f"{t()} catalog.view items=3",
             f"{t()} newsletter.open", f"{t()} cart.update items=1"]
    signal = {
        "double_charge": ["payment.captured order=O amount=48", "payment.captured order=O amount=48 (91 s later)"],
        "refund_request": ["order.cancelled by=customer", "refund.requested order=O"],
        "payment_failed": ["payment.declined order=O code=54 card_expired"],
        "not_delivered": ["carrier.scan status=in_transit hub=north", "carrier.no_scan_since days=11"],
        "damaged": ["support.photo_uploaded order=O tag=broken_item"],
        "wrong_item": ["warehouse.pick order=O sku=SKU-Z9", "order.line order=O sku=SKU-A1"],
        "account_locked": ["auth.failed x5", "account.locked reason=failed_attempts"],
        "account_takeover": ["auth.success device=new country=elsewhere", "account.email_changed", "account.password_changed"],
    }
    lines = rng.sample(noise, 2) + ([f"{t()} {s}" for s in signal[kind]] if kind != "none" else
                                    [f"{t()} payment.captured order=O", f"{t()} carrier.scan status=delivered"])
    rng.shuffle(lines)
    return lines


# ---- messages: pools written by the generator model -----------------------------
POOL_PROMPT = """You write realistic customer-support messages for testing a router.

Write {n} different messages, 1-3 sentences each, in a customer's own voice, varied in tone and detail. Never use the words of the description literally.

{instruction}

Answer with JSON only: {{"messages": ["...", ...]}}"""


def instruction(kind, mode):
    if mode == "clear":
        return f"The customer's situation: {DESC[kind]}. They describe it plainly."
    if mode == "misleading":
        return (f"The customer believes their situation is: {DESC[NEIGHBOUR[kind]]}. They describe it that "
                f"way, sincerely and plainly (they are mistaken, but the message must read as that situation).")
    return "The customer writes that something is wrong and asks for help, but gives no specifics at all."


def pools():
    out = HERE / "data/messages"
    out.mkdir(exist_ok=True)
    jobs = [(k, m) for k in CLASSES for m in ("clear", "misleading")] + [("any", "absent")]
    for kind, mode in jobs:
        n = D["pool_size"] * (2 if mode == "absent" else 1)
        prompt = POOL_PROMPT.format(n=n, instruction=instruction(kind, mode) if kind != "any" else instruction(None, "absent"))
        body = json.dumps({"model": D["generator"]["model"], "messages": [{"role": "user", "content": prompt}],
                           "response_format": {"type": "json_object"}}).encode()
        req = urllib.request.Request(D["generator"]["endpoint"], data=body, headers={
            "Authorization": "Bearer " + os.environ["OPENROUTER_API_KEY"], "Content-Type": "application/json"})
        with urllib.request.urlopen(req, timeout=240) as r:
            raw = json.loads(r.read())
        content = raw["choices"][0]["message"]["content"]
        msgs = json.loads(content[content.index("{"):content.rindex("}") + 1])["messages"]
        (out / f"{kind}.{mode}.json").write_text(json.dumps({"prompt": prompt, "response": raw, "messages": msgs}, indent=1))
        print(kind, mode, len(msgs))


def cases():
    pool = {p.name.rsplit(".", 1)[0]: json.loads(p.read_text())["messages"] for p in (HERE / "data/messages").glob("*.json")}
    truth = {}
    for cond in D["conditions"]:
        rng = random.Random(cond["seed"])
        lines = []
        for kind in CLASSES:
            for i in range(cond["per_class"]):
                # Channel modes, per the condition's noise model.
                if cond["noise"] == "independent":
                    modes = {ch: rng.choices(["clear", "absent", "misleading"], weights=cond["weights"])[0]
                             for ch in ("message", "ledger", "events")}
                else:
                    if rng.random() < cond["joint_misleading"]:
                        modes = {ch: "misleading" for ch in ("message", "ledger", "events")}
                    else:
                        modes = {ch: rng.choices(["clear", "absent"], weights=cond["otherwise"])[0]
                                 for ch in ("message", "ledger", "events")}
                shown = {ch: (kind if m == "clear" else NEIGHBOUR[kind] if m == "misleading" else "none")
                         for ch, m in modes.items()}
                msg_key = "any.absent" if modes["message"] == "absent" else f"{kind}.{modes['message']}"
                message = rng.choice(pool[msg_key])
                case_id = f"E3-{cond['id']}-{kind}-{i + 1}"
                case = {"id": case_id, "goal": "Determine the customer's situation.",
                        "message": message,
                        "ledger": ledger(shown["ledger"], rng) if shown["ledger"] != "none" else ledger("none", rng),
                        "events": events(shown["events"], rng)}
                truth[case_id] = {"condition": cond["id"], "truth": kind, "modes": modes, "shown": shown}
                lines.append("Case: " + json.dumps(case))
        (HERE / f"data/cases.{cond['id']}.jobs").write_text(
            f"# E3 cases, condition {cond['id']} ({cond['noise']} noise), generated by generate.py (seed {cond['seed']}).\n"
            + "\n".join(lines) + "\n")
    (HERE / "data/truth.json").write_text(json.dumps(truth, indent=1) + "\n")
    print({c["id"]: c["per_class"] * len(CLASSES) for c in D["conditions"]})


if __name__ == "__main__":
    {"pools": pools, "cases": cases}[sys.argv[1]]()
