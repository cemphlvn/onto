# E5b — notes during the experiment

- Jev judged all 660 stream messages on `data/message10.onto`, no
  failures (497k input tokens).
- The AUROC helper was replaced by the rank formula (Mann-Whitney) before
  the run and unit-checked (1.0, 0.0, 0.5, 0.5 on known inputs).
- Consistency: the secondary stream's L0 reproduced E5 exactly (1.816 per
  call, 95.3%, learner errors 113 and 132 over 10 orders), so E5b is
  comparable to E5.
- While the run was going, a flaw in the simulator's design was noticed:
  the learner's temperature is fitted on the **last 25%** of the labels
  it has, so its calibration set depends on stream position (after new
  situations arrive it is dominated by cases the learner gets wrong).
  This is a candidate cause of the errors on known situations that no
  policy here addresses. It is not changed in E5b (the plan fixed the
  procedure); E5c tests it.
