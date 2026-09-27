# Judges on one frame: Jev against a TKG model (tkgd)

The flat 40-intent frame of `large.onto` (no `grouped by`), the 22
labelled tickets of `large.jobs` with `expected` moved to `labels.json`
(so the label cannot reach a judge), and a Turkish copy of both. The
Turkish copy keeps every object and arrow id, so answers are compared
arrow by arrow.

| file | what |
|---|---|
| `flat-en.onto`, `en.jobs` | the English frame and tickets |
| `flat-tr.onto`, `tr.jobs` | the same, Turkish text (arrow instructions, `about`, the frame question, the tickets) |
| `labels.json` | ticket id → the intent a person chose |
| `run.sh` | runs one judge on one language: `bench/run.sh <tr\|en> <judge-model>` |
| `score.py` | scores the frame records against the labels |

Judges:
- `jev-latest`: TypeSafe Jev over HTTP (`TYPESAFE_API_KEY`).
- `tkgd:tkg-suyu-d128`: the TKG unit model `birim_lm` d128 (1.89 M
  parameters, fonto Adım 10e) behind fonto's `tkgd` server
  (`POST /tkg/puanla`). Read by calibrated likelihood: the ticket is the
  user's message; each intent's Turkish text is scored as the assistant's
  reply, minus the same reply with no ticket (PMI); softmax over the 40.
  `none_of_these` gets no mass.
- `tkgd-raw:tkg-suyu-d128`: the same without calibration (plain
  log-likelihood; favours short, a-priori likely replies).

Measures: **top-1** (the judge's most probable arrow is the label);
**engine** (the walk took the labelled arrow at the 0.6 threshold; the
rest escalated or went wrong); **answered** and their precision; wall.
Chance top-1 is 1/40: 22 tickets give P(≥ 3 right) = 0.017,
P(≥ 4) = 0.002, P(≥ 5) = 0.0002.

## Pre-registration (2026-09-27, written before any live run)

Predictions:
1. Jev, English: 21–22/22 top-1 (it was 22/22 on 2026-09-24 with
   `expected` still in the case).
2. Jev, Turkish: 20–22/22 top-1; at most 2 below its English run.
3. tkgd calibrated, Turkish: **2–6/22** top-1. The model was trained on
   Turkish chat (question → answer); it picks topic words but writes no
   coherent content (fonto `deney/tkg/README.md`, "Kullanıcı gözüyle ilk
   tur"). Word overlap between a ticket and an intent's text (`iki kez
   ücret`, `hesabını kapatmak`, `şifre`) should carry a few tickets through
   PMI; nothing else will.
4. tkgd raw: 0–2/22 (surface-form competition).
5. tkgd engine: its softmax over sums of per-unit log-probabilities will be
   peaked, so it will answer many tickets above 0.6 with low precision
   (≤ 50%): a model that is confident without being right.

Decision rule: calibrated top-1 **≥ 4/22** (p < 0.01 against chance) counts
as a signal that the TKG model carries decision-relevant meaning on this
frame; below it, not. One seed and one frame: a signal, not evidence.

## Results

(Filled in after the runs; predictions above are not edited.)
