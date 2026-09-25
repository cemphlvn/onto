#!/usr/bin/env python3
"""E4 data: messages (generator model, separate train and test calls in
different styles) and structured cases (E3's templates, new seeds), with
truth by construction; plus Jev job files for the teacher and baseline.

python3 generate.py messages   # OPENROUTER_API_KEY: writes data/messages/
python3 generate.py cases      # deterministic: data/{split}.{channel}.jobs, data/truth.json
"""
import json
import os
import random
import sys
import urllib.request
from pathlib import Path

HERE = Path(__file__).resolve().parent
D = json.loads((HERE / "data/design.json").read_text())
E3 = json.loads((HERE / "../e3-independent-perspectives/data/design.json").read_text())
CLASSES = [c["id"] for c in E3["classes"]]
DESC = {c["id"]: c["describe"] for c in E3["classes"]}

# Templates copied from deney/e3-independent-perspectives/generate.py (unchanged).
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



PROMPT = """You write realistic customer-support messages for testing a router.

Write {n} different messages, 1-3 sentences each (longer if the style asks for it), in a customer's own voice, varied in tone and detail. Never use the words of the description literally.
Style: {style}

{instruction}

Answer with JSON only: {{"messages": ["...", ...]}}"""


def ask(prompt):
    body = json.dumps({"model": D["generator"]["model"], "messages": [{"role": "user", "content": prompt}],
                       "response_format": {"type": "json_object"}}).encode()
    req = urllib.request.Request(D["generator"]["endpoint"], data=body, headers={
        "Authorization": "Bearer " + os.environ["OPENROUTER_API_KEY"], "Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=300) as r:
        raw = json.loads(r.read())
    content = raw["choices"][0]["message"]["content"]
    return raw, json.loads(content[content.index("{"):content.rindex("}") + 1])["messages"]


def messages():
    for split in ("train", "test"):
        spec = D["messages"][split]
        for kind in CLASSES + ["vague"]:
            n = spec["vague"] if kind == "vague" else spec["per_situation"]
            inst = ("The customer writes that something is wrong and asks for help, but gives no specifics at all."
                    if kind == "vague" else f"The customer's situation: {DESC[kind]}. They describe it plainly.")
            got, calls = [], []
            while len(got) < n:
                want = min(20, n - len(got))
                prompt = PROMPT.format(n=want, style=spec["style"], instruction=inst)
                raw, msgs = ask(prompt)
                calls.append({"prompt": prompt, "response": raw})
                got += msgs[:want]
            (HERE / f"data/messages/{split}.{kind}.json").write_text(
                json.dumps({"calls": calls, "messages": got}, indent=1))
            print(split, kind, len(got))


def cases():
    truth = {}
    for split in ("train", "test"):
        rows = {"message": [], "ledger": [], "events": []}
        for kind in CLASSES + ["vague"]:
            for i, m in enumerate(json.loads((HERE / f"data/messages/{split}.{kind}.json").read_text())["messages"]):
                cid = f"E4-{split}-message-{kind}-{i + 1}"
                truth[cid] = {"split": split, "channel": "message", "truth": None if kind == "vague" else kind}
                rows["message"].append("Read: " + json.dumps({"id": cid, "goal": "Determine the customer's situation.", "message": m}))
        s = D["structured"]
        rng = random.Random(s[f"{split}_seed"])
        per = s[f"{split}_per_situation"]
        for ch in ("ledger", "events"):
            for kind in CLASSES:
                for i in range(per):
                    shown = "none" if rng.random() < s["absent_share"] else kind
                    body = ledger(shown, rng) if ch == "ledger" else events(shown, rng)
                    cid = f"E4-{split}-{ch}-{kind}-{i + 1}"
                    truth[cid] = {"split": split, "channel": ch, "truth": None if shown == "none" else kind}
                    rows[ch].append("Read: " + json.dumps({"id": cid, "goal": "Determine the customer's situation.", ch: body}))
        for ch, lines in rows.items():
            (HERE / f"data/{split}.{ch}.jobs").write_text("\n".join(lines) + "\n")
            print(split, ch, len(lines))
    (HERE / "data/truth.json").write_text(json.dumps(truth, indent=1) + "\n")


if __name__ == "__main__":
    {"messages": messages, "cases": cases}[sys.argv[1]]()
