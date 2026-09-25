# E5 — measurement plan (written before the simulation ran)

## Question

Does the self-building loop resolve more intents per model call than Jev
alone, within the same accuracy and escalation budget? And how does the
share the loop takes over grow with the cases it has seen (the growth
curve asked about after E0b)? (`docs/12` §6.)

## Stream (no new model calls)

E4's 540 customer messages (360 training + 180 test), each with Jev's
recorded probabilities and its truth by construction. Two phases: the
first 270 cases come from 6 situations and vague messages; after them
all 8 situations appear (the 2 late ones: `wrong_item`,
`account_takeover`, arriving new). 10 stream orders (seeds 0 to 9).

## Policies

- **Jev alone (baseline):** every case is a Jev call; Jev's top answer is
  followed when its probability is at least 0.6 (onto's gate), otherwise
  the case goes to a person (who answers the truth).
- **Self-building loop:** a CPU learner (E4's closed learner, fixed
  λ = 1e-4, temperature on the last 25% of its data) retrained every 30
  cases on everything labelled so far. For each case: if the learner is
  trained and its top probability is at least 0.8, it answers (no model
  call); otherwise Jev is called: if Jev's probability is at least 0.6,
  Jev answers and its probabilities become a training label; otherwise
  a person answers and the truth becomes a training label.

## Measures

- **resolved per model call:** cases resolved (by any means) / Jev calls.
- **accuracy** of the resolved answers against truth (a person is right).
- **escalations to a person.**
- **learner share over the stream** (the growth curve), in windows of 30
  cases; fitted by a saturating exponential s(n) = s∞ (1 − e^(−n/τ)) and
  by a power law s(n) = a n^b over the first phase; the better fit by
  squared error is reported, with both.
- **take-over of new situations:** after the late situations arrive, the
  share of their cases the learner answers, by how many of them it has
  seen.
- Means over the 10 orders with the range.

## Predictions

- **Q1:** the loop resolves at least 2× more cases per model call than
  Jev alone over the whole stream (E4: the learner took 77% at a 0.8
  gate once trained).
- **Q2:** the loop's accuracy is within 1 point of Jev alone, with no more
  escalations to a person.
- **Q3:** the growth curve saturates (the exponential fits better than the
  power law) at a share near E4's 70 to 80%.
- **Q4:** new situations are taken over after a few dozen of their cases,
  learned from Jev's answers and people's, with no model change.

## Threats

- Jev's recorded judgments already include the late situations' arrows
  (in onto the open world would first learn them): the loop's Jev calls
  on a late situation's first cases are therefore optimistic.
- The person is assumed to answer the truth, instantly and at no cost
  beyond counting.
- Messages are E4's (synthetic; two styles). No retraining cost counted
  in model calls (the learner trains on the CPU).
