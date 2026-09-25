#!/usr/bin/env python3
"""E4, exploratory (not in the plan): does calibration fitted on the
training style transfer to the test style? And the combined system: the
CPU learner takes the cases it is surest of, Jev the rest.

python3 explore.py   # after learn.py; writes results/e4.explore.json
"""
import json
import numpy as np
import learn as E

truth = E.load_truth()
lab = lambda cid: E.L[truth[cid]["truth"] or "none"]
train, test = E.read_jobs("train.message"), E.read_jobs("test.message")
tr_ids, te_ids = list(train), list(test)
tr_text = [E.text_of(train[i], "message") for i in tr_ids]
te_text = [E.text_of(test[i], "message") for i in te_ids]
y_tr = np.array([lab(i) for i in tr_ids]); y_te = np.array([lab(i) for i in te_ids])
m = E.train_closed(tr_text, np.eye(len(E.LABELS))[y_tr])
idx = np.random.default_rng(7).permutation(len(tr_ids))
va = idx[:int(len(tr_ids) * 0.25)]
Pv = E.predict_closed(m, [tr_text[i] for i in va])
Pt = E.predict_closed(m, te_text)
J = E.jev("test.message"); PJ = np.array([J[i][0] for i in te_ids])
out = {"calibration_transfer": {
    "validation_same_style": {"accuracy": round(float((Pv.argmax(1) == y_tr[va]).mean()), 3), "ece_top": round(E.ece_top(Pv, y_tr[va]), 3),
                              "mean_top_p": round(float(Pv.max(1).mean()), 3)},
    "test_other_style": {"accuracy": round(float((Pt.argmax(1) == y_te).mean()), 3), "ece_top": round(E.ece_top(Pt, y_te), 3),
                         "mean_top_p": round(float(Pt.max(1).mean()), 3)}}}
# Combined: CPU takes cases with top p >= gate, Jev the rest.
rows = []
for gate in (0.5, 0.7, 0.8, 0.9, 0.95):
    take = Pt.max(1) >= gate
    pred = np.where(take, Pt.argmax(1), PJ.argmax(1))
    rows.append({"gate": gate, "cpu_share": round(float(take.mean()), 3),
                 "cpu_accuracy_on_its_share": round(float((Pt.argmax(1)[take] == y_te[take]).mean()), 3) if take.any() else None,
                 "combined_accuracy": round(float((pred == y_te).mean()), 3), "jev_calls_saved": round(float(take.mean()), 3)})
out["combined"] = rows
out["jev_alone_accuracy"] = round(float((PJ.argmax(1) == y_te).mean()), 3)
(E.HERE / "results/e4.explore.json").write_text(json.dumps(out, indent=1) + "\n")
print(json.dumps(out, indent=1))
