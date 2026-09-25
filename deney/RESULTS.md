# deney — results ledger

Headline numbers per experiment, kept on every experiment branch (machine-readable twin: `results.json`). Full measurements are in each experiment's README and `results/`.

| id | question | headline | status |
|---|---|---|---|
| E0 | share of recorded switches a CPU learner could take (capability ceiling), loose resonance rule | ceiling 55.2% of 737 switches; both constraints handled 99.6% | superseded by E0b (resonance rule too loose) |
| E1 | Jev's calibration per switch on labelled demo cases | 144/145 right; ECE 0.003; only 21 of 2889 switches between p 0.1 and 0.9 | labels too clear to measure calibration |
| E2 | boundary cases; does a same-information loop label them | uncertain calls 24% (E1 8%); share of A by level 1.00 · 0.94 · 0.46 · 0.09 · 0.00; loop flagged 0/6 disagreements; 8/9 surprises were a policy contradiction | same-information loops are not label sources; synthetic labels must be adjudicated against the whole frame |
| E0b | E0 with only loops over different information as label sources | ceiling 14.7%; labels bind (69.9% if handled) over vocabulary (26.3%) | labels bind, not vocabulary |
| E3 | do loops over different information catch errors; U vs number of perspectives | independent: slope −1.08 [−1.43, −0.87] vs ln ē −1.11, three perspectives worth 2.97, detection 92%; correlated: slope −0.11, k_eff ≈ 1 | law: U ≈ ē^k_eff; k_eff ≈ k for independent information, ≈ 1 for shared |
| E4 | a CPU switch learner against Jev | structured: CPU 100% at 0.02–0.07 ms (Jev 100% / 90% at 320 ms); messages: CPU 95.6% (Jev 99.4%), 77% taken at gate 0.8 with system at 99.4%; unseen situations 1.9% | structured: CPU; free text: CPU under a gate; open vocabulary: not lexical |
| E5 | self-building loop: intents per model call within the same accuracy | loop 1.82× per call at 95.3% (Jev 1.00× at 99.8%); errors: new situations 132, early overconfidence 113; guard 1.30× at 98.3% | the learner needs a novelty signal (E5b) |
| E5b | a novelty signal and minimum evidence for the loop | fresh stream: L3 1.84× at 96.9% (Jev 100%); detector AUROC 0.84 but 2% prevalence; known-situation errors 16.7 of 20.7 per order; guard 1.22× at 98.9% | the threshold must come from a target error rate; known-situation errors dominate |

## Across experiments

- **Law (E3):** undetected error of a loop U ≈ ē^k_eff; k_eff ≈ k when perspectives see different information, ≈ 1 when they share it (E2 is the extreme).
- **Labels (E0b, E2):** only evidence, people, and loops over different information are label sources; a generator's label must be adjudicated against the whole frame.
- **Calibration (E1, E2, E4):** measure top-answer calibration on boundary cases; a calibration fitted on one distribution is a claim about that distribution, and its direction (under or over) matters.
- **CPU learner (E4, E5):** structured switches fully; free text under a gate; not open vocabulary; in a loop it needs a novelty signal, or it answers new situations silently wrong.
- **Method:** plans, labels and cases committed before any judgment; one branch per experiment; errors decomposed by source; exploratory analyses labelled and not tuned on the same data.
