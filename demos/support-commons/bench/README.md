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

2026-09-27, M4 Mac, one seed, one frame. Mean label rank: 1 = always
top; chance 20.5 of 40, standard error over 22 tickets ≈ 2.5. MRR chance
0.107. Rank and MRR were added to the scorer after the first tkgd run,
when top-1 turned out to sit at chance (not pre-registered).

| judge | lang | top-1 | engine right / answered | mean label rank | MRR | wall |
|---|---|---|---|---|---|---|
| Jev | en | 22 | 21 / 21 | 1.0 | 1.000 | 0.7 s |
| Jev | tr | 22 | 22 / 22 | 1.0 | 1.000 | 0.9 s |
| tkgd raw, d128 6 000 steps | tr | 2 | 2 / 21 | 19.3 | 0.175 | 5.0 s |
| tkgd calibrated, d128 6 000 steps | tr | 1 | 1 / 16 | 12.1 | 0.205 | 6.1 s |

Against the predictions: 1 and 2 held (Jev lost nothing in Turkish). 3
failed: calibrated top-1 is 1/22, below 2–6, and below the decision rule
(≥ 4), so **no top-1 signal**. 4 held (2/22). 5 held: tkgd answered 16
tickets above 0.6 with 6% precision.

Why top-1 fails: both readouts collapse onto one favourite intent
(calibrated: `cancel_subscription` on 9 of 22 tickets, p up to 0.999;
raw: `delete_account` on 16). The sum of per-unit log-ratios over 15–25
units has a wide spread, so one option wins the softmax outright.

Yet the calibrated ranking carries meaning: the label's mean rank is 12.1
against 20.5 by chance (z ≈ 3.4).

**Does it learn?** The same readout across the TKG models:

| model (fonto) | training | mean label rank | MRR |
|---|---|---|---|
| `kucuk-d1` (floor) | 1 500 steps, constant LR | 21.5 | 0.109 |
| `kucuk-d4`, `kucuk-d16` | 1 500, constant LR | 18.5, 20.4 | 0.148, 0.140 |
| `izgara` d64 (data 25/50/100%) | 1 500, constant LR | 18.1 / 17.7 / 18.7 | 0.120 / 0.132 / 0.145 |
| `izgara` d128 (data 25/50/100%) | 1 500, constant LR | 17.9 / 20.0 / 17.7 | 0.094 / 0.158 / 0.165 |
| `izgara` d256 (100%) | 1 500, constant LR | 22.5 | 0.084 |
| `suyu-d128` | 1 500, warmup + cosine | 15.5 | 0.235 |
| `suyu-d128` | 3 000 | 12.2 | 0.247 |
| `suyu-d128` | 4 500 | **11.1** | 0.219 |
| `suyu-d128` | 6 000 | 12.1 | 0.205 |

- The 1 500-step grid is within two standard errors of chance at every
  size and data share; d256 (the least converged, fonto Adım 10c) is the
  worst.
- The 10e run (warmup + cosine decay, 8.4 epochs) gets there: rank falls
  15.5 → 12.2 → 11.1 over 1 500 → 4 500 steps and stops at 6 000,
  matching fonto's reading that 10e has used up its data. The step from
  1 500 to 3 000 is the one outside the noise; the later ones are not.
- So the model learns something the frame can use, it is ordinal and weak
  (the label lands in the top quarter), and training gives it; size alone
  does not.

**Is the stop at 6 000 real?** Paired bootstrap over the 22 tickets'
label ranks (20 000 resamples):

| from → to | Δ rank | 95% interval |
|---|---|---|
| 1 500 → 4 500 | −4.4 | [−7.8, −1.2] |
| 1 500 → 3 000 | −3.4 | [−6.9, 0.0] |
| 3 000 → 4 500 | −1.0 | [−3.1, +0.6] |
| 4 500 → 6 000 | +1.0 | [−0.5, +2.5] |
| 6 000 → 6 000 with number classes (`s13-tur-t0`) | +0.3 | [−4.7, +5.5] |

Training helps up to 4 500 steps; from 4 500 to 6 000 the rank does not
improve (a slight, non-significant worsening), in line with the 10e
validation loss (lowest 1.91 at 5 000, 2.015 at 6 000; train–validation
gap 0.15 → 0.55) and with the learning rate of the cosine schedule
(1.6e-4 at 4 500, 1e-5 at 6 000). The pipeline is deterministic
(`s13-degersiz-t0` reproduces 10e rank for rank), so seed noise is still
unmeasured; changing only the number format moves single tickets by up to
±5 ranks, so per-ticket readings are unreliable (`s13-tur-t0` reached 4/22
top-1 with the same mean rank). The `s13-rakam` models no longer load
(three digit units were added after they were trained).

Cold start of the tkgd worker (fonto `ff2db54`, tokenizer table cache),
three interleaved runs each on a quiet machine: 22.3–22.9 s → 7.1–7.3 s
wall (setup 19 s → 4 s).

Next: a per-unit (length-normalised) PMI readout against the collapse;
the HLM models of fonto T11 once `birim_hlm.py` can serve (it has no
server mode yet); Adım 18 (HD state + ridge readout) once trained.

## Readout against the collapse: per-unit PMI (pre-registration, 2026-09-27, before any run)

Calibrated top-1 collapses onto one favourite intent because the score is
a **sum** of per-unit log-ratios over 15–25 units: its spread grows with
the option's length, so one long option can win the softmax outright.
`tkgd-norm:` divides each option's PMI by its unit count (mean per-unit
log-ratio). Same model (`suyu-d128`, 6 000 steps, and 4 500), same 22
tickets; nothing else changes. This readout was chosen after seeing the
collapse, so its result is exploratory, not a test of the original rule.

Predictions:
1. The collapse breaks: no intent is top-1 on more than 5 of 22 tickets
   (sum PMI: 9 of 22).
2. Top-1 rises to 3–6/22; mean label rank stays within ±2 of the sum
   readout (12.1 at 6 000, 11.1 at 4 500), since ranking was already the
   part that worked.

Decision: if top-1 ≥ 4/22 on both checkpoints, `tkgd-norm:` becomes the
default TKG readout for this bench (and T15's M3 uses it); otherwise the
sum readout stays and the collapse is reported as a property of the model.

Results (2026-09-27, same 22 tickets):

| readout | model | top-1 | mean label rank | MRR | most frequent top-1 | engine answered |
|---|---|---|---|---|---|---|
| sum PMI | d128, 6 000 | 1 | 12.1 | 0.205 | `cancel_subscription` 9 | 16 (6% right) |
| per-unit PMI | d128, 6 000 | 4 | 12.2 | 0.298 | `cancel_subscription` 8 | 0 |
| per-unit PMI | d128, 4 500 | 3 | 11.5 | 0.255 | `cancel_subscription` 12 | 0 |

Against the predictions: 1 **failed**: the collapse stays (8 and 12 of
22 on one intent). 2 held: top-1 3–4, rank within ±1. Decision rule
(≥ 4/22 on both checkpoints): **not met**, so the sum readout stays the
default. Per-unit PMI does change one thing: its softmax is flat, so the
engine answers nothing above 0.6 instead of answering 16 tickets with 6%
precision (confidently wrong → no answer).

Since the collapse survives length normalisation, it is not a length
effect. Next hypothesis, not tested here (a further readout picked after
seeing results would be a forking path): the prior is scored with **no**
context, which is unlike any user message; contextual calibration (Zhao
et al. 2021) uses a content-free *input* ("N/A") instead. A neutral
Turkish user turn as the baseline would test whether `cancel_subscription`
wins by being generically likely after any request.

## D26 — prior from neutral user turns (pre-registration, 2026-09-27, before any run)

Hypothesis (from the per-unit result above): `cancel_subscription` wins
because the prior is scored with **no** context, and any user message at
all raises it. Contextual calibration (Zhao et al. 2021) takes the prior
from content-free *inputs*, averaged over several. `tkgd-nb:` scores each
option after three fixed neutral Turkish user turns, chosen now:
"Merhaba.", "Bir sorum var.", "Yardımcı olabilir misiniz?"; the prior is
the mean of the three log-probabilities, and the score is the summed PMI
against it (as `tkgd:`). Same models (d128 at 6 000 and 4 500 steps), same
22 tickets.

Predictions:
1. The collapse breaks: no intent is top-1 on more than 5 of 22 tickets.
2. Top-1 ≥ 4/22 on both checkpoints; mean label rank ≤ 11.0 on both.

Decision: if 1 and 2 both hold, `tkgd-nb:` becomes the default TKG
readout for this bench and for T15's M3; if 1 fails, the collapse is a
property of the model under every calibration tried, and the bench keeps
the sum readout. This is the last readout variant tried on these 22
tickets; any further one needs a new, held-out ticket set.

D26 results (2026-09-27):

| readout | model | top-1 | mean label rank | MRR | most frequent top-1 | engine right / answered |
|---|---|---|---|---|---|---|
| sum PMI (`tkgd:`) | d128, 6 000 | 1 | 12.1 | 0.205 | `cancel_subscription` 9 | 1 / 16 |
| neutral prior (`tkgd-nb:`) | d128, 6 000 | 4 | **8.0** | 0.371 | `cancel_subscription` 6 | 3 / 18 |
| sum PMI (`tkgd:`) | d128, 4 500 | 1 | 11.1 | 0.219 | — | — |
| neutral prior (`tkgd-nb:`) | d128, 4 500 | 4 | **8.8** | 0.366 | `cancel_subscription` 7 | 3 / 16 |

Paired bootstrap of the label rank, neutral prior against sum PMI:
- 6 000 steps: −4.1 [−7.0, −1.6]
- 4 500 steps: −2.3 [−5.2, +0.4]

Against the predictions: 1 **failed**, narrowly: the collapse shrinks but
stays, 6 and 7 of 22 on one intent against a limit of 5. 2 held (top-1 4
and 4; rank 8.0 and 8.8 ≤ 11.0).

Decision, as written: 1 failed, so the sum readout stays the bench's
default and the decision readout for T15's M3. `tkgd-nb:` is reported next
to it as a secondary readout, because it does move the ranking (−4 ranks at
6 000). Most of the collapse was the empty-context prior; what remains
belongs to the model.

This was the last readout tried on these 22 tickets; a further one needs a
new, held-out ticket set.
