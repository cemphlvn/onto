# E5b — measurement plan (written before any generation or judgment)

## Question

E5's loop answered situations it had never seen with confidence (132
errors), and was overconfident early (113). Does a novelty signal the
learner computes on the CPU, with a minimum of evidence before acting,
bring the loop within 1 point of Jev alone's accuracy while resolving
more intents per model call?

## Streams

- **Primary (fresh novelty).** E4's 540 messages (8 known situations and
  vague ones) plus 120 new ones from 2 situations never used before
  (`subscription_cancel`, `address_change`; 30 in E4's training style and
  30 in its test style each, from the generator model). The first 330
  cases are known situations only; the new situations arrive among the
  remaining 330. Jev judges all 660 afresh on `data/message10.onto` (the
  Message frame with the two new arrows). 10 orders.
- **Secondary (E5's stream).** E5's own stream and Jev judgments,
  reported for comparison only: the detector was designed after seeing
  E5, so it is not evidence there.

## Policies (all reported)

| id | the learner may answer when | |
|---|---|---|
| L0 | p ≥ 0.8 (E5's loop) | reference |
| L1 | L0 and it has at least 90 labels | minimum evidence |
| L2 | L0 and the case is not novel | novelty |
| L3 | L1 and L2 | both |
| L4 | E5's guard (minimum evidence, 10% audits, suspension on a new label) | reference |

**Novelty (fixed rule, no tuning):** a case is novel when its highest
cosine similarity (hashed words and bigrams, E4's features) to the
learner's training texts is below the 5th percentile of the training
texts' own highest similarity to each other (leave one out), recomputed
at each retraining. Otherwise as E5: retrain every 30 cases; Jev
answers at p ≥ 0.6 (its answer becomes a label), else a person (the
truth).

## Measures

As E5 (resolved per model call, accuracy, escalations to a person,
learner share, errors by source), plus **detector quality**: for cases
reaching a learner not yet trained on their situation, the AUROC of the
novelty score (1 − highest similarity) between new-situation cases and
known ones.

## Predictions

- **R1:** L3's accuracy is within 1 point of Jev alone's, on the primary
  stream.
- **R2:** L3 resolves at least 1.5× more intents per model call than Jev
  alone.
- **R3:** the detector's AUROC is at least 0.8.
- **R4:** L3's learner errors on new situations fall by at least 80%
  against L0.

## Threats

As E5 (Jev's arrows exist before the open world would learn them; a
person answers the truth, free). The new situations are adjacent to
known ones (a cancellation near a refund request; an address change near
delivery problems), which makes novelty harder to detect by word overlap.
