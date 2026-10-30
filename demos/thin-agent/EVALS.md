# Architecture evals: where onto's architecture checks can be measured

Research notes, 2026-10-30. Question: which public evaluations could
measure (a) conformance checking with a standard and a functor, (b)
`onto discover` proposing a role map, (c) ranking repair options.

Checked 2026-10-30: `SAEroConRepo/systems/JabRef/` holds the 3.7
sources, `JabRef 3.7 Intended Architecture - Final.xls`, **`JabRef 3.7
Violations - Final.xls`** (labelled violations), session notes, and a
second architecture (`JabRef-3.7-0002/jabref.xam`, `jabref-generic.txt`).
TEAMMATES and ProM not yet checked.

## Fit, best first

| eval | what it gives | tests in onto | fit |
|---|---|---|---|
| **SAEroCon repository** (JabRef, TEAMMATES, ProM; workshops 2016–2019) — github.com/sebastianherold/SAEroConRepo | an expert's intended architecture (modules, allowed dependencies) and the code-to-module mapping | intended architecture → a standard; mapping → roles; derive + functor → violations, against reflexion-model results. Discover as role proposer vs the expert mapping, against s4rdm3x baselines (HuGMe, NBAttract) | **best**: same shape as `derive.py` + `roles.toml`; needs a Java reader |
| **Microservice infrastructure patterns** (arXiv 2603.23073, ICSA 2026) | 190 GitHub repos, human-annotated pattern instances; LLM F1 0.09–0.70 | discover against a pattern library: which pattern does a repo instantiate | good: it is the experiment we ran on onto, with labels |
| **Ground-truth architectures** (Garcia et al. ICSE 2013, ASE 2013; Lutellier et al. ICSE SEIP 2015) — Bash, ITK, Chromium, Hadoop, ArchStudio in RSF | verified recovered architectures (clusterings) | role proposal at scale (MoJoFM-style agreement) | partial: recovery is clustering, onto maps into declared roles |
| **ADR violations** (arXiv 2602.07609, ICSA 2026 distinguished paper) | 980 ADRs from 109 projects, LLM violation detection | `rule` invariants (judged) vs structural `via`/`never` (proved) | partial: mostly decisions not tied to code; dataset link unconfirmed |
| **Arcan smells** (782 Java/Python projects, IST 2026; Zenodo 15488631 for hub-like) | cyclic, hub-like, unstable dependency smells | smells as `never` invariants | weak: graph checks, little for categories to add |
| **ArchBench** (arXiv 2603.17833) | ADR generation, architecture Q&A, serverless tasks, leaderboard | — | no: generative tasks |

## Repair ranking has no public ground truth

No dataset pairs a violation with the repair people chose. Two ways to
build one:

1. **Seeded violations** (the mutation method of arXiv 2609.18994): take
   a conforming architecture, inject a known violation, ask for repairs;
   the inverse mutation is the answer. Cheap, labels exact, but
   synthetic.
2. **Mined repairs**: in SAEroCon systems' history, commits after which
   a violation disappears; the commit is the chosen repair. Real, but
   costly to label.

## Plan

1. SAEroCon JabRef: a Java reader for `derive.py` (packages, imports,
   effects), the intended architecture as a standard, the expert mapping
   as roles. Measure violations vs the published results; then discover
   vs the expert mapping (does it beat HuGMe/NB? where does partiality
   hide violators?).
2. Microservice patterns: pattern library from the paper's definitions,
   discover per repo, F1 against the labels.
3. Repair ranking: seeded violations on the conforming SAEroCon models.

## Published scores and how they are normalized (read 2026-10-30)

None of these is an L1 or L2 vector norm. They are count ratios and
normalized edit distances; L1 against L2 only becomes a real choice in
calibration (below).

| eval | score | normalization | published numbers |
|---|---|---|---|
| MicroPAD, microservice patterns (arXiv 2603.23073) | P, R, A, F1 per pattern; one overall figure | unit: repo × pattern, binary (190 × 9 = 1710), threshold 5/10. Overall figure is **micro** (pooled confusion matrix TP 114, FP 116, FN 234, TN 1246), though the paper does not name it. Macro F1 from Table III ≈ 0.28 (our arithmetic) | overall A 79.5%, P 49.6%, R 32.8%, F1 39.5%; per pattern F1 0.09–0.70 (GPT 5 nano). F1 vs prevalence r = 0.74 |
| Naive Bayes orphan adoption (Olsson et al., JSS, arXiv 2109.09525) | P, R, F1 = harmonic mean | per run, against the expert mapping; 50 000 random initial sets per system across sizes; reported as **running median** F1 with 25th–75th percentiles, plus Mann-Whitney-style tests with standardized effect sizes. P and R formulas not printed; reading: P = correct / automatically mapped, R = correct / orphans (our inference). No per-system number table, only plots and a ranking (Table 5: NB > CA > IR/LSI in all 8 systems) | sizes (Table 1): JabRef 1015 entities, 6 modules, 15 module deps; TEAMMATES 779, 15, 28; ProM 261, 4, 5 |
| MoJoFM (Lutellier et al., ICSE SEIP 2015) | similarity of two partitions, % | `(1 − mno(A,B) / max mno(∀A,B)) × 100`: minimum Move/Join operations, divided by the worst case. A normalized edit distance (a count of unit operations: L1-like). Not symmetric | e.g. ACDC: Bash 41/59, ITK 59/56, Chromium 63/70, ArchStudio 60/77, Hadoop 24/41 (include / symbol deps) |
| a2a (same) | architecture similarity, % | `(1 − mto(A1,A2) / (aco(A1) + aco(A2))) × 100`; mto = cluster adds + removes, entity adds + removes + moves; aco = the cost of building each from empty | ACDC 64–88; all techniques mostly 60–88, so a2a compresses differences |
| c2c, c2ccvg (same) | cluster overlap; share of covered ground-truth clusters | `|ci ∩ cj| / max(|ci|, |cj|)`; coverage = clusters over a threshold (50/33/10%) / ground-truth clusters | — |
| TurboMQ (same) | modularization quality | **not normalized**: a sum of cluster factors, grows with the number of clusters (0.06–503); not comparable across systems | — |

For our runs:

- Report micro **and** macro, and every class's prevalence: micro is
  dominated by common classes (MicroPAD: frequent patterns carry F1).
- To compare with Olsson et al., follow their protocol: medians over
  many initial-set sizes, P over auto-mapped, R over orphans. onto
  discover maps with an empty initial set; that point sits at the left
  end of their curves.
- Calibration of Jev's p is where L1 against L2 matters: **ECE** is a
  binned mean absolute gap |accuracy − confidence| (L1); the **Brier
  score** is the mean squared error of p (L2) and decomposes into
  reliability, resolution and uncertainty; log loss punishes confident
  errors hardest. Report ECE and Brier, with a reliability diagram.

## Result 1: JabRef 3.7, reader + expert mapping (2026-10-30)

Layer 1 of three: roles fixed to the expert mapping (HUSACCT model
0001), so every error is the reader's or the rule semantics'. Script:
scratch `jabref_eval.py` (regex reader: imports, qualified names,
simple names resolved against imports and the package).

- Rules: 17 (not allowed to use, only-module-may-use for Swing/AWT and
  java.sql/Oracle, back call and skip call over GUI > Logic > Model).
- Gold: 286 violating dependencies (statement level) = **36 unique
  (class, class-or-library) pairs**; we grade pairs, since the reader
  sees no statements.
- **TP 33 · FP 1 · FN 3 → P 0.971 · R 0.917 · F1 0.943.**
- The 3 FN are all HUSACCT "Indirect" calls (through a superclass or a
  call chain); an import reader cannot see them.
- The 1 FP is a real violation the gold misses:
  `logic.l10n.Localization:32` calls
  `javax.swing.JComponent.setDefaultLocale` by its qualified name,
  without an import. Counted against us anyway.
- An earlier run gave F1 0.72; the gap was our grader not stripping
  HUSACCT's `xLibraries.` prefix, not the reader.
- Not yet onto: rules were applied in Python. Next, the same check as
  a standard + functor, so onto is what decides.

Small sample: 36 pairs, one system. With pairs, one error moves P or R
by about 3 points.

## Result 2: JabRef 3.7, discover proposes the mapping (2026-10-30)

Layer 2: no initial set. Source: 120 packages, 1089 package dependencies
read from code, each described by code only (package name, up to 15
class names). Target: the 6 HUSACCT modules described by the expert
documentation, verbatim (JabRef wiki, High Level Documentation, 2016),
including its rule "packages not yet structured count as gui". Graded at
class level against the expert mapping (844 classes), P over mapped
classes, R over all (Olsson et al.'s convention). Script: scratch
`layer2.py`.

**Meaning only** (target = complete graph, so structure does not prune):

| | value |
|---|---|
| coverage (classes mapped) | 0.885 (93 of 120 packages) |
| P / R / F1 (micro) | **0.908 / 0.803 / 0.852** |
| macro F1 over 6 modules | 0.652 |
| package-level oracle (every package to its expert majority module) | 0.989 |
| GUI · Logic · Model F1 | 0.846 · 0.893 · 0.872 (n = 344, 383, 98) |
| CLI · Pref · Globals F1 | 0.516 · 0.450 · 0.333 (n = 8, 10, 1) |

Reference: NBAttract's median F1 on JabRef is ≈ 0.90 at the smallest
initial sets (Olsson et al. Fig. 7, read off the plot), with training
on those sets; we use none.

Calibration of Jev's p for the chosen module (93 packages, correct =
the expert's majority module):

| p bin | packages | accuracy |
|---|---|---|
| 0.2–0.4 | 8 | 0.12 |
| 0.4–0.6 | 44 | 0.98 |
| 0.6–0.8 | 38 | 0.95 |
| 0.8–1.0 | 3 | 1.00 |

Brier 0.178, ECE (5 bins) 0.356. **Underconfident but sharply
discriminative**: almost every error sits below p = 0.4, so a threshold
at 0.4 is the analogue of NBAttract's 0.9. The p is a choice among six
options, so 0.5 means "clearly ahead", not "a coin flip"; it needs a
monotone recalibration (isotonic or Platt, fit on another system) before
it can be read as a probability.

Where it fails:

- **Small modules carry macro F1 down**: CLI, Pref and Globals are 19
  classes in all; wrong packages pulled into them (`gui.preftabs` →
  Pref, `gui.importer.fetcher` → CLI) cost them precision. Olsson et al.
  warn about the same (minority classes).
- **The documentation's default rule is not used**: packages under
  `gui.*` were sent to Logic or CLI at p ≈ 0.3; the target `about` says
  unstructured packages count as gui, but the judge reads meaning, not
  rules.
- **27 packages left outside**. 12 the judge rejected ("none of these",
  0.32–0.66). **15 were never judged**: they have no outgoing
  dependencies, and discover's role rule maps a terminal only to a
  terminal (`crates/onto-core/src/discover.rs:47`); no module of a
  dependency architecture is a terminal. About 44 classes (5%) of
  recall are lost to a heuristic made for decision categories.

**Structure + meaning** (target = allowed module dependencies only):
pending; Jev returned 529 (overloaded) twice.

## Lessons that transfer to other benchmarks

General (apply to every eval):

1. **Grade in the unit the gold labels and the reader can both see**,
   and report the collapse: JabRef's 286 violation rows are 36 class
   pairs. One error then moves a score ~3 points; say so.
2. **Read every FP and FN by hand before trusting a number.** Our first
   F1 (0.72) was a prefix mismatch in our own grader.
3. **Gold is not truth.** Keep a strict score and an adjudicated one;
   the `Localization` FP is a real violation the gold misses.
4. **Separate the layers**: fix the human inputs to the gold to measure
   the reader; replace one input at a time (roles from discover, then
   descriptions) to price each component.
5. **Inputs a user would have are fair; anything derived from the gold
   is a leak.** The system's documentation is fair; splitting packages
   where the expert split them is not. Measure the oracle bound the chosen
   granularity imposes instead (0.989 here: the best any package-level
   mapper can do, knowing the answer).
6. **Follow the benchmark's own conventions for comparability** (P over
   mapped, R over all; median over initial sets) and add what it lacks:
   macro per class with prevalence, and calibration (ECE, Brier,
   reliability bins).
7. **Check onto's heuristics against the domain**: the terminal rule
   and path semantics (a functor accepts a forbidden edge if an allowed
   path exists) were built for decision categories; dependency graphs
   need direct edges and no role pruning.
8. **The judge needs retries**: discover sends its calls in parallel
   and fails the whole run on one 529.

Special cases, per benchmark:

- **TEAMMATES (SAEroCon)**: 15 modules, so the judge chooses among more
  options and p will run lower; xlsx violation report with reviewer
  notes (adjudication is partly done). Same two layers apply.
- **ProM (SAEroCon)**: no violation labels; layer 2 (mapping) only. 4
  modules and 261 entities: small, most packages can be named directly.
- **MicroPAD (microservice patterns)**: evidence lives in deployment
  artifacts (Dockerfiles, compose, Kubernetes), not imports: a new
  reader. Unit is repo × pattern, labels are contributors'
  self-reports, prevalence 10–35% per pattern and F1 tracks prevalence
  (r = 0.74): report per pattern, macro and micro, with prevalence.
  Patterns become target categories; "does the repo instantiate it" is
  whether a functor exists, not a mapping score.
- **Garcia et al. ground truths (Bash, ITK, Chromium, Hadoop,
  ArchStudio)**: the modules are not given; they are what recovery
  finds. onto maps into declared modules, so only a hybrid fits:
  declare the ground-truth modules without their mapping and score the
  mapping with MoJoFM / a2a. Mostly C/C++: another reader.
- **ADR violations**: decisions in prose; most are not about code
  structure. That is the judged `rule` invariant, a different layer from
  proved `via`/`never`.

## Result 3: ablation over the thin-agent parts (JabRef 3.7, 2026-10-30)

Code: `evals/jabref/` (`layer2.py`, `ablation.py`); commit 8680a74 adds
`--no-roles`, `--goal`/`--instructions` and the resilient judge client.
Graded at class level (844 classes), P over mapped, R over all.

Full configuration **F** = path constraint (allowed module
dependencies) + no roles + architecture framing + module descriptions
from JabRef's documentation + packages described by their class names.
Each row removes one part. Repeats: F twice (identical), the best
configuration twice (F1 0.978 / 0.977): Jev is near-deterministic here,
**noise ≈ 0.001 F1**.

F: coverage 0.892 · P 0.988 · R 0.882 · F1 0.932 · macro F1 0.694

| role | part removed from F | ΔF1 | Δmacro F1 | Δcoverage |
|---|---|---|---|---|
| substrate | role pruning back on | **−0.116** | −0.155 | −0.083 |
| substrate | path constraint removed (complete target) | **+0.046** | **+0.174** | +0.093 |
| blueprint | framing: architecture → default (merger) | −0.025 | −0.020 | −0.039 |
| blueprint | module descriptions → module names only | −0.011 | −0.002 | −0.029 |
| agent input | package description → package name only | −0.004 | −0.001 | −0.006 |

The framing matters far more without the path constraint: meaning
only, default framing F1 0.873 → architecture framing **0.978**
(+0.105).

Best configuration, **meaning only + no roles + architecture framing**:

| | coverage | P | R | F1 | macro F1 |
|---|---|---|---|---|---|
| discover | 0.985 | 0.986 | 0.970 | **0.978** | **0.868** |
| documentation rules only | 1.000 | 0.969 | 0.969 | 0.969 | 0.800 |
| doc rules pinned + discover + doc default | 1.000 | 0.972 | 0.972 | 0.972 | 0.865 |
| package-level oracle | | | | 0.989 | |
| NBAttract, smallest initial sets (Olsson et al., read off Fig. 7) | | | | ≈ 0.90 | |

Calibration (raw → Platt, leave-one-out): ECE 0.06 → 0.02 here, against
0.356 → 0.049 under the default framing. The right blueprint fixes most
of the calibration before any recalibration.

What the ablation says about the architecture:

1. **Substrate heuristics must fit the domain.** Role pruning, built for
   decision categories, is the largest single loss (−0.116).
2. **Checking and mapping are separate steps.** The path constraint
   adds no precision (0.986 → 0.988) and costs recall: real code
   violates its architecture, and a mapping that must respect the
   allowed dependencies leaves the violators out. Map by meaning first,
   then check the mapping with the functor (reflexion modelling's order).
3. **The blueprint carries the domain.** Framing is the largest positive
   lever (+0.105 without the constraint) and also fixes calibration.
4. **Descriptions add little on JabRef** (−0.011, −0.004), because its
   names mirror the architecture; documentation rules alone reach
   0.969. Systems whose names do not mirror their modules (TEAMMATES is
   the next test) will show whether descriptions carry more there.
5. **Calibration measured here is in-sample** (leave-one-out within
   JabRef); fit it on one system and apply it to another before relying
   on it.

## Result 4: TEAMMATES 5.110, names hidden (2026-10-30)

Code: `evals/sysmdl/` (generic over s4rdm3x `.sysmdl`, the expert
mappings Olsson et al. evaluated). Configuration: meaning only, no
roles, architecture framing. 812 classes, 34 packages, 15 modules;
package-level oracle 1.0.

**The benchmark leaks the answer through package names.** In 7 of the 8
Olsson et al. systems the expert mapping is a package-prefix regex
(ProM 4/4 rules, Commons Imaging 20/21, TEAMMATES 14/15, Sweet Home 3D
8/9), and module names are mostly package names. With paths visible, a
package-level mapper is reading the answer. So each system runs twice:
**visible** (package path + class names) and **hidden** (shuffled ids,
class names only), the second standing in for a system whose names do
not mirror its architecture.

| blueprint (module descriptions) | names | accuracy | coverage | P | R | macro P | macro R | macro F1 | largest confusion |
|---|---|---|---|---|---|---|---|---|---|
| module names only | visible | 0.648 | 0.877 | 0.739 | 0.648 | 0.916 | 0.939 | 0.878 | testDriver → ui.automated: 101 |
| module names only | hidden | 0.560 | 1.000 | 0.560 | 0.560 | 0.593 | 0.692 | 0.586 | testDriver → ui.automated: 206 |
| + the expert document, restated (`teammates.doc.json`) | visible | 0.708 | 0.837 | 0.846 | 0.708 | 0.964 | 0.949 | 0.937 | testDriver → ui.controller: 85 |
| + the expert document | hidden | **0.818** | 1.000 | 0.818 | 0.818 | 0.804 | 0.858 | 0.787 | testDriver → ui.automated: 65 |
| + one convention: all test code is testDriver (`teammates.doctest.json`, marked as ours) | visible | **1.000** | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | — |
| + the convention | hidden | **0.937** | 1.000 | 0.937 | 0.937 | 0.862 | 0.879 | 0.845 | common.dataTransfer → storage.entity: 37 |
| string baseline (package name ↔ module name tokens, sees names) | visible | 0.669 | 0.820 | 0.815 | 0.669 | | | 0.688 | |

**Reading the micro numbers.** P is over mapped classes, R over all
(Olsson et al.). At full coverage each wrong class is one false positive
(for the module it went to) and one false negative (for its own), so
micro P = R = F1 = accuracy: hidden + convention has TP 761, FP 51,
FN 51. Micro P and R separate only when the mapper abstains (visible:
coverage 0.84–0.88, P > R). The errors show per module: in hidden +
convention, common.dataTransfer has P 1.000 / R 0.431 and
storage.entity P 0.245 / R 1.000, the same 37 transfer objects taken
for entities; logic.api (6 classes, the facade) P = R = 0.

**Reporting standard from here on**: accuracy and coverage; micro P/R
only next to coverage; per-module P/R/F1; macro P, R and F1; the
largest confusions.

- With names visible, every error was test code: the expert puts all
  tests in testDriver, Jev put tests with the module they test. One
  stated convention takes visible to 1.000. The same lesson as JabRef's
  "the rest is gui": **convention modules must be written into the
  blueprint**.
- With names hidden, the expert's documentation is worth +0.258 F1 and
  the convention another +0.119: **the blueprint is where the human
  domain knowledge enters, and on a system without telling names it is
  the largest lever measured so far.**
- The 5 remaining hidden errors sit on real boundaries:
  `common.datatransfer.*` → storage.entity (transfer object vs persisted
  entity, p 0.62–0.77), `client.scripts.util` → common.util,
  `client.scripts.scalabilitytests` → testDriver (p 0.93: by our own
  convention it is a test; the expert says client.scripts, so the
  blueprint and the gold disagree), and `logic.api` → ui.automated
  (wrong).
- Calibration on 34 packages is too few points to fit a map (Platt LOO
  ECE moves 0.07 → 0.09 hidden); per-system calibration needs labels
  pooled across systems, or the human answers collected at the gate.

## Result 5: ArgoUML r13713, a clean test (2026-10-30)

**Protocol.** Blueprint written only from ArgoUML's Cookbook in the same
revision (`trunk@13713` = git `00eb8d17d9`), purpose and layer
statements close to verbatim, package and class names removed, and
**frozen in commit 1ce6f0b before any ArgoUML run**. Disclosed: before
writing it I had seen the `.sysmdl` module list and its first 7 mapping
rules; no rule content went into the blueprint. Anything added after
seeing errors would be a separate "expertise value" row; none was
added.

Sources: `src_new` + `src/model` (1503 classes read). The expert mapping
covers 820 classes in 54 of 78 packages; ungraded classes are left out,
as in Olsson et al. 19 modules, of which 4 (profile, help,
taskManagement, configuration) have no classes: distractors. Package-
level oracle 1.0.

| blueprint | names | accuracy | coverage | macro P | macro R | macro F1 | packages right | largest confusion |
|---|---|---|---|---|---|---|---|---|
| **Cookbook (clean)** | **hidden** | **0.856** | 1.000 | 0.681 | 0.718 | 0.663 | 45/54 | model → diagrams: 81 |
| **Cookbook (clean)** | **visible** | **0.987** | 1.000 | 0.924 | 0.894 | 0.900 | | moduleLoader → application: 6 |
| module names only | hidden | 0.757 | 1.000 | 0.602 | 0.582 | 0.573 | | model → diagrams: 81 |
| module names only | visible | 0.943 | 1.000 | 0.866 | 0.863 | 0.829 | | critics → taskManagement: 35 |
| string baseline (sees names) | — | 0.404 | 0.428 | 0.470 | 0.464 | 0.454 | | |

(Micro P = R = accuracy at full coverage; the string baseline abstains,
P 0.943 over the 43% it maps.)

- **Blueprint value, clean**: +0.099 accuracy / +0.090 macro F1 hidden,
  +0.044 / +0.071 visible. Calibration hidden: ECE 0.098 raw, 0.040 after
  Platt (leave-one-out, in-sample).
- **One decision is most of the gap**: `org.argouml.model` (81 classes,
  the model facade) → diagrams at p 0.55. Without it hidden accuracy
  would be ≈ 0.955. Class-level grading weighs package decisions by
  size; package accuracy (45/54) shows the decision count.
- **Errors sit at low p**: 7 of 9 wrong packages were judged at p ≤ 0.55
  (0.19–0.55); a gate there would send them to a person.
- **Two errors are the blueprint's, by our own rule**:
  `uml.reveng.java` → javaCodeGeneration (the Cookbook puts Java reverse
  engineering in the Java subsystem; the expert puts it in
  reverseEngineering) and `application.api` → application at p 0.86
  (the Cookbook ties that package to the module loader by name, which
  our no-package-names rule removed). Blueprint and gold disagree in
  the first; the second is a cost of hiding names.

Across the three systems, hidden names (no help from paths): TEAMMATES
0.818 (expert document only, the clean equivalent), ArgoUML 0.856
(clean). With names visible and the documentation: JabRef 0.978,
TEAMMATES 0.771 (tests unconvened), ArgoUML 0.987.

## Result 6: the sealed test (2026-10-31)

Five systems never touched before: ProM 6.9, Lucene (trunk at
r1075001), Commons Imaging 1.0-alpha2, Ant (trunk at r584500), Sweet
Home 3D r2382. Blueprints written blind from each system's own
documentation (four by agents that never saw the mapping files) and
frozen before any run; records collected and committed unscored;
calibration choice frozen before (`evals/calibration/`,
PREREGISTRATION.md, choice.json, test_result.json).

### Mapping (configuration as Result 5; accuracy at class level)

| system | visible | hidden | macro F1 hidden | string baseline (sees names) | largest hidden confusion |
|---|---|---|---|---|---|
| ProM | 0.916 | 0.680 | 0.460 | 1.000 | contexts → framework: 35 |
| Lucene | 0.992 | 0.886 | 0.927 | 0.972 | util → index: 23 |
| Commons Imaging | 1.000 | 0.827 | 0.913 | 0.962 | tiff → common: 28 |
| Ant | 0.973 | 0.577 | 0.770 | 0.974 | taskdefs → ant: 127 |
| Sweet Home 3D | 1.000 | 1.000 | 1.000 | 0.000* | — |
| **mean (5)** | **0.976** | **0.794** | 0.814 | | |

\* the string baseline's tokenizer cannot split `sweetHome3DModel`
into a token matching `model`: an artifact of the baseline, not a
result.

- With names visible, discover matches the expert on all five
  (0.92–1.00), but so does the string baseline wherever it can tokenize:
  on these benchmarks visible names carry the answer.
- **With names hidden, the blind result is 0.794 on average**, from
  1.000 (Sweet Home 3D) to 0.577 (Ant). Training systems with clean
  blueprints scored 0.818–0.856; the sealed mean is lower, with two
  systems weak: Ant (taskdefs → ant core: the documentation describes
  tasks and the core in the same terms) and ProM (contexts → framework:
  its documentation is a forum and an announcement, the thinnest
  blueprint of all).

### Calibration (pre-registered)

358 sealed decisions, judge right 81.6% (package level, visible and
hidden pooled).

| map | Brier | log loss | ECE | coverage @0.9 | precision @0.9 | asked |
|---|---|---|---|---|---|---|
| **identity (pre-registered choice)** | **0.0994** | 0.319 | 0.074 | 0.360 | **1.000** | 229 |
| Platt, pull 1 (post hoc) | 0.0955 | 0.298 | 0.021 | 0.587 | 0.986 | 148 |
| isotonic (post hoc, best Brier) | 0.0950 | 0.293 | 0.019 | 0.497 | 0.989 | 180 |
| others (post hoc) | 0.095–0.102 | | 0.019–0.034 | | | |

- **The pre-registered choice held up on its own terms**: Brier
  differences across all twelve maps stay within 0.007, as the training
  folds predicted; no map is clearly better by Brier.
- **ECE is where calibration shows** (post hoc): every fitted map cuts
  ECE from 0.074 to about 0.02, on test as on training (0.080 → 0.057).
  Brier barely moves because it is dominated by how well the judge
  separates right from wrong, which a monotone map cannot change.
- **At the gate the choice has a price** (post hoc): raw p adopts only
  36% and asks about 229 of 358, never wrong; Platt adopts 59% and asks
  148, wrong about 3 times. At the 10 : 1 cost the gate itself assumes,
  Platt costs ≈ 148 + 30 = 178 against 229. On the training folds the
  same arithmetic favoured identity.
- **Lesson for the next pre-registration** (not applied here): select
  on the decision the map serves, the expected cost at the gate's
  threshold, not on Brier. A map that is better calibrated without
  being better on Brier can still change how many questions a person
  gets.

## Result 7: the pipeline end to end, and the expertise rows (2026-10-31)

### Gate + a person (from the records, no new judge calls)

The gate adopts the judge's choice when raw p ≥ t; every other package
is asked, and the person is simulated by the expert mapping (an upper
bound: a real person may err). Class-level accuracy, sealed systems,
names hidden (`evals/calibration/gate_human.py`):

| threshold | accuracy | packages asked |
|---|---|---|
| judge alone | 0.794 | 0 |
| 0.5 | 0.926 | 56 / 179 (31%) |
| **0.7** | **0.977** | 89 / 179 (50%) |
| 0.9 | 1.000 | 140 / 179 (78%) |

With names visible the same gate asks far less (Commons Imaging 1 of 41
at 0.9, Lucene 1 of 23 at 0.7). Against mapping by hand (every package
asked), the gate at 0.7 halves a person's work on systems whose names
do not help, and nearly removes it where they do.

### Expertise rows (post hoc; the clean scores stay as in Result 6)

Owner statements added after seeing the sealed errors (`*.expert.json`,
marked `[owner]`), names hidden:

| system | clean (documentation) | + owner statements | largest confusion left |
|---|---|---|---|
| ProM | 0.680 / macro F1 0.460 | **0.840** / 0.727 | contexts → framework: 24 (was 35) |
| Ant | 0.577 / 0.770 | 0.649 / 0.711 | taskdefs → ant: 127 (unchanged) |

Ant's 127-class package did not move even with an explicit statement.
Its description to the judge was its **first 15 class names in
alphabetical order**: `AbstractCvsTask, AbstractJarSignerTask, Ant,
AntStructure, Antlib, AntlibDefinition, Apt, Available, ...`: names
that read like the core, while the tasks that make the package what it
is (Copy, Javac, Jar) never appear. Hypothesis: the error is in how
the substrate renders the source, not in the blueprint.

**Tested and refuted (same day, post hoc):** described instead by its
size and the classes other units reference most (`--sample=referenced`:
"127 classes, the most used of them: Execute, LogStreamHandler,
MatchingTask, Java, ..."), the package still went to the core (accuracy
0.577 → 0.572). Its most-used classes read like infrastructure too. The
cause stays open: Ant's tasks package holds engine-like classes, and
the core module's description ("the base task class") draws them.

**For the next pre-registered round** (not applied to any result
above): select the calibration map on the gate's expected cost (Result
6). The referenced sample (`--sample=referenced`) is available but did
not help where it was tried.

Ant note: s4rdm3x's Ant jar held no optional tasks (its `.sysmdl` says
no files match `optional`); our source tree has them, and they are
graded by the same rules, so Ant here is larger (736 classes) than
Olsson et al.'s (515 entities).

## Result 8: the judges compared, all behind onto's Judge contract (2026-10-31)

Same frames, same blueprints, same substrate (`--no-roles`, meaning
only, architecture framing); only `--judge-model` changes
(`evals/calibration/leaderboard.py`). Package level: one decision, one
point; a decision counts as right only when the judge's choice is
adopted and matches the expert. Each judge against Jev on the decisions
both answered, exact McNemar test:

| judge | decisions | accuracy, judge / Jev | names hidden, judge / Jev | only judge right / only Jev right | p | probabilities | spent |
|---|---|---|---|---|---|---|---|
| **Gemini 3.1 Pro** | 410 (6 systems) | **0.907** / 0.846 | **0.878** / 0.776 | 37 / 12 | **0.0005** | stated | $10.25 |
| Grok 4.3 | 202 (3 systems) | 0.782 / 0.782 | 0.644 / 0.703 | 21 / 21 | 1.0 | stated (no log-probabilities, though listed) | $2.20 |
| Qwen 3.5 397B | 653 (8 systems) | 0.799 / 0.838 | 0.629 / 0.704 | 26 / 51 | 0.006 | letter log-probabilities | $2.41 |

On the 202 decisions all four answered: Gemini 0.851 (Brier 0.058, ECE
0.043), Jev 0.782 (0.112, 0.095), Grok 0.782 (0.101, 0.037), Qwen 0.688
(0.189, 0.166).

- **A general-purpose frontier model beats Jev on this task**, most
  where names are hidden (+0.10), at about $25 per 1,000 decisions
  with ~4,900 input tokens each. Jev's price per decision was not known
  here, so accuracy per dollar is missing for it.
- **The thin-agent claim held**: the judge was swapped by one flag; the
  substrate, the gate, the records and the checks did not change.
  Whichever judge wins, onto keeps the parts that make it usable: the
  gate, calibration from records, and the functor check.
- **Incomplete**: the OpenRouter key reached its total limit mid-run.
  Gemini has 12 of 15 runs (missing Ant visible and hidden, JabRef),
  Grok 6 of 15. A local judge (Qwen3.5-2B on device) was built and
  started, then dropped by decision. Claude Fable was not run (about
  $36 at this prompt size).
- Contamination is not ruled out: these systems are old and public;
  names hidden reduces it, it does not remove it.
