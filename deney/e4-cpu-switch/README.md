# E4 — a CPU switch learner against Jev

**Question.** Can a switch learner that runs on a CPU take switches from
Jev without losing accuracy or calibration, and on which kinds of switch:
structured records, free text within known situations, or situations it
has never seen (open vocabulary)? (`docs/12` §4, §6.)

**Lineage.** Branch `deney/e4-cpu-switch`, from
`deney/e3-independent-perspectives` at `dfa4443`. Plan
(`MEASUREMENT.md`), design, generator and all data were committed before
any judgment; `NOTES.md` was written during the run.

## Design

- **Data, truth by construction.** E3's 8 support situations plus "no
  signal". Structured cases (ledger, event log) from E3's templates with
  new seeds: 400 train, 200 test per channel. Customer messages from the
  generator model, **train and test from separate calls in different
  styles** (train: plain web-form messages, 360; test: hurried mobile,
  non-native or long detailed messages, 180).
- **Jev** judged every test case (the baseline) and every training
  message (the teacher).
- **Learners** (numpy 2.4.3, scipy 1.17.1, on an Apple M4): multinomial
  logistic regression over hashed words and bigrams (2^15), trained on
  log loss, temperature-scaled on a validation split; trained on truth,
  or **distilled** from Jev's probabilities with no truth at all. For
  open vocabulary, a model over (message, arrow description) pairs,
  trained with two situations held out entirely and tested on them
  (4 folds covering all 8).

```sh
cd deney/e4-cpu-switch
python3 generate.py messages && python3 generate.py cases   # already run; committed
./run.sh                                                    # TYPESAFE_API_KEY
python3 learn.py && python3 explore.py                      # results/e4*.json
```

## Results

| switches | Jev | CPU learner, trained on truth | CPU learner, distilled from Jev |
|---|---|---|---|
| **ledger** (structured) | 100% · top-answer ECE 0.014 · 320 ms per call | **100% · 0.000 · 0.07 ms per case** | |
| **event log** (structured) | 90% [85.5, 94] · 0.087, overconfident | **100% · 0.000 · 0.02 ms** | |
| **messages** (free text) | **99.4%** [98.3, 100] · 0.010 | 95.6% [92.2, 98.3] · 0.074, underconfident · 0.02 ms | 95.0% [91.7, 97.8] · 0.082 |
| … share it can take at Jev's accuracy | | **75.6%** | 75.0% |
| **unseen situations** (open vocabulary) | 97.5–100% | **1.9%** (chance 11%); 96.8% on the seen ones | |

Combined system on messages (exploratory): the learner takes the cases
where its probability is at least a gate, Jev the rest.

| gate | learner's share | learner's accuracy on it | combined accuracy | Jev calls saved |
|---|---|---|---|---|
| 0.7 | 83% | 98.0% | 98.3% | 83% |
| **0.8** | **77%** | **99.3%** | **99.4%** (Jev alone: 99.4%) | **77%** |
| 0.9 | 70% | 100% | 99.4% | 70% |

Predictions: **P1** (structured: at Jev's level, under 1 ms) held, and
on event logs the learner beat Jev. **P2** (free text) held for accuracy
(below Jev) and coverage (75.6% ≥ 50%), and missed on calibration
(0.074 > 0.05, see below). **P3** (distillation within 5 points of
truth) held: 95.0% against 95.6%, with no truth label (the teacher
agreed with truth on 100% of training messages). **P4** (open
vocabulary fails) held, harder than predicted: below chance.

## What it says

1. **Structured switches belong on the CPU.** Records and logs: the
   learner matched or beat Jev at about 1/10,000 of the latency, and was
   calibrated where Jev was overconfident (event logs).
2. **Free-text switches: a CPU learner can take most of them, if it may
   defer.** With a gate it took 77% of messages and the system stayed at
   Jev's accuracy, while Jev answered only the 23% the learner was least
   sure of. Distillation alone (Jev's probabilities, no truth) did as
   well as truth labels here, because the teacher was right on these
   cases; where the teacher is wrong it would copy its errors, which is
   why E1 to E3's labels matter.
3. **Open vocabulary is not a lexical problem.** A situation the
   learner never saw, known only by its description, it essentially
   never chose (1.9%): it cannot connect a new description to messages
   without a model of meaning. Jev did it at 97.5–100%. Open-vocabulary
   switches stay with Jev (or need a text encoder, which this CPU
   learner deliberately does not have).
4. **Against E0b.** The switches a CPU learner can take are the closed,
   labelled ones; it takes structured ones fully and free-text ones
   under a gate; open ones not at all. That is E0b's joint table, now
   measured rather than declared.

## Measurement notes (after the experiment)

- **Calibration is a claim about a distribution.** The learner's
  validation ECE in the training style was 0.046; on the test style,
  which turned out easier, 0.074, underconfident. The *direction* of a
  calibration error matters as much as its size: underconfidence costs
  coverage (Jev answers more), overconfidence costs errors.
- **Structured results are partly by construction**: templates are easy
  for a lexical model; real records vary more. The event-log gap (Jev
  90%) is real for Jev on these templates.
- **The learner is a lower bound**: hashed words and bigrams with a
  linear model, the simplest CPU learner. A small text encoder is the
  next step for free text and the only route to open vocabulary on a CPU.
- **One teacher, one generator, synthetic messages**; test style shift
  was deliberate.
