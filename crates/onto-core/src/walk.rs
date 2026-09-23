//! Two-tier walks.
//!
//! At each object the walker reads its decision frame: the outgoing arrows
//! whose `require` holds in the walk's state. The frame's primitive says how
//! a System-1 [`Judge`] decides it:
//!
//! - **choice**: one arrow, or "none of these";
//! - **noul**: each arrow's condition on its own; when several hold, a fork
//!   judgment says whether to pursue them all in parallel or only the best;
//! - **score**: one ordered scale; the level reached picks the arrow.
//!
//! Open and closed frames are judged alike. When nothing fits (or the judge
//! is unsure) the step escalates to a System-2 [`Proposer`], whose
//! suggestions stay provisional.

use serde_json::Value;

use std::collections::BTreeSet;

use crate::category::{ArrowId, Category, Closure, Gate, ObjId, Primitive};
use crate::path::Path;

/// What the models see at each step.
#[derive(Clone, Debug)]
pub struct WalkState {
    /// Free-text goal or context for the models.
    pub goal: String,
    /// Structured facts about the case; `require` clauses read it.
    pub state: Value,
    /// Capability tokens the walk holds (set and cleared by arrow effects).
    pub tokens: BTreeSet<String>,
    pub at: ObjId,
    pub path: Path,
}

/// A probability per candidate arrow, plus one for "none of these".
#[derive(Clone, Debug)]
pub struct Distribution {
    pub arrows: Vec<f32>,
    pub none_of_these: f32,
    /// The model's own certainty, when it reports one (Jev does). Gates the
    /// fast path instead of the raw top probability.
    pub confidence: Option<f32>,
}

impl Distribution {
    /// Index of the most likely arrow and its probability, or `None` when
    /// "none of these" is at least as likely as every arrow.
    pub fn top(&self) -> Option<(usize, f32)> {
        let (i, p) = self
            .arrows
            .iter()
            .copied()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(&b.1))?;
        (p > self.none_of_these).then_some((i, p))
    }
}

/// A System-1 answer for one frame, shaped by the frame's primitive.
/// Indices refer to the candidate arrows in the order they were given.
#[derive(Clone, Debug)]
pub enum Answer {
    Choice(Distribution),
    Noul {
        /// P(condition holds) per candidate.
        holds: Vec<f32>,
        /// P(several holding arrows are independent aspects to pursue in
        /// parallel, rather than competing readings). `None` if not asked.
        fork: Option<f32>,
    },
    Score {
        /// Probability per level, candidates ordered by level.
        levels: Vec<f32>,
        confidence: Option<f32>,
    },
}

impl Answer {
    pub fn confidence(&self) -> Option<f32> {
        match self {
            Self::Choice(d) => d.confidence,
            Self::Score { confidence, .. } => *confidence,
            Self::Noul { .. } => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "snake_case")
)]
pub enum Escalation {
    OpenFrame,
    NoneOfThese,
    LowConfidence,
    /// An all-join cannot complete: a sibling ended elsewhere.
    IncompleteJoin,
    /// A gate join's authority branch ended without arriving.
    BlockedByGate,
}

impl Escalation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OpenFrame => "open_frame",
            Self::NoneOfThese => "none_of_these",
            Self::LowConfidence => "low_confidence",
            Self::IncompleteJoin => "incomplete_join",
            Self::BlockedByGate => "blocked_by_gate",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Decision {
    /// Follow candidate `index`. `alternatives` are other candidates that
    /// also held (noul) but were not pursued.
    Follow {
        index: usize,
        p: f32,
        alternatives: Vec<(usize, f32)>,
    },
    /// Pursue every listed candidate as its own branch.
    Fork {
        branches: Vec<(usize, f32)>,
        fork_p: f32,
    },
    Escalate(Escalation),
}

/// The System-1 gate, shared by every walker. `answer` is the judge's
/// answer for the frame, `None` when no arrow was eligible to ask about.
/// Open frames are judged like closed ones: an existing arrow that fits is
/// followed. When nothing fits, the reason says which kind of gap it is:
/// `none_of_these` for a closed frame (its MECE claim failed), `open_frame`
/// for a frame already known to be incomplete. `can_fork` is the code-side
/// guard (branch budget, depth); the model's fork judgment only counts when
/// it is true.
pub fn decide(
    closure: Closure,
    answer: Option<&Answer>,
    threshold: f32,
    can_fork: bool,
) -> Decision {
    let nothing_fits = match closure {
        Closure::Closed => Escalation::NoneOfThese,
        Closure::Open => Escalation::OpenFrame,
    };
    let Some(answer) = answer else {
        return Decision::Escalate(nothing_fits);
    };
    match answer {
        Answer::Choice(d) => match d.top() {
            Some((index, p)) if d.confidence.unwrap_or(p) >= threshold => Decision::Follow {
                index,
                p,
                alternatives: Vec::new(),
            },
            Some(_) => Decision::Escalate(Escalation::LowConfidence),
            None => Decision::Escalate(nothing_fits),
        },
        Answer::Noul { holds, fork } => {
            let mut held: Vec<(usize, f32)> = holds
                .iter()
                .copied()
                .enumerate()
                .filter(|(_, p)| *p >= threshold)
                .collect();
            held.sort_by(|a, b| b.1.total_cmp(&a.1));
            match held.len() {
                0 if holds.iter().any(|p| *p > 1.0 - threshold) => {
                    Decision::Escalate(Escalation::LowConfidence)
                }
                0 => Decision::Escalate(nothing_fits),
                1 => Decision::Follow {
                    index: held[0].0,
                    p: held[0].1,
                    alternatives: Vec::new(),
                },
                _ => {
                    let fork_p = fork.unwrap_or(0.0);
                    if can_fork && fork_p >= 0.5 {
                        Decision::Fork {
                            branches: held,
                            fork_p,
                        }
                    } else {
                        let (index, p) = held.remove(0);
                        Decision::Follow {
                            index,
                            p,
                            alternatives: held,
                        }
                    }
                }
            }
        }
        Answer::Score { levels, confidence } => {
            let Some((index, p)) = levels
                .iter()
                .copied()
                .enumerate()
                .max_by(|a, b| a.1.total_cmp(&b.1))
            else {
                return Decision::Escalate(nothing_fits);
            };
            if confidence.unwrap_or(p) >= threshold {
                Decision::Follow {
                    index,
                    p,
                    alternatives: Vec::new(),
                }
            } else {
                Decision::Escalate(Escalation::LowConfidence)
            }
        }
    }
}

/// Candidates for a frame in the order judges see them: eligible arrows in
/// frame order, or by level for score frames.
pub fn candidates(
    cat: &Category,
    at: ObjId,
    case: &Value,
    tokens: &BTreeSet<String>,
) -> Vec<ArrowId> {
    let mut c = cat.eligible(at, case, tokens);
    if cat.object(at).frame.primitive == Primitive::Score {
        c.sort_by_key(|a| cat.arrow(*a).level);
    }
    c
}

/// System 1: a fast, calibrated judgment over a closed frame.
pub trait Judge {
    fn judge(&mut self, cat: &Category, state: &WalkState, candidates: &[ArrowId]) -> Answer;
}

/// A new arrow suggested by the System-2 proposer. The target may be an
/// existing object or a new one.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Proposal {
    pub arrow: String,
    pub src: String,
    pub dst: String,
    /// What the proposed arrow means: when to follow it.
    pub about: String,
    pub rationale: String,
    /// Capability effects the proposal would have (reviewed against the
    /// capability declarations like any arrow's).
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Vec::is_empty"))]
    pub ensures: Vec<String>,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Vec::is_empty"))]
    pub revokes: Vec<String>,
}

/// System 2: generates structure the frame is missing.
pub trait Proposer {
    fn propose(&mut self, cat: &Category, state: &WalkState) -> Vec<Proposal>;
}

#[derive(Clone, Debug)]
pub enum Step {
    Followed {
        arrow: ArrowId,
        p: f32,
    },
    Escalated {
        reason: Escalation,
        proposals: Vec<Proposal>,
    },
}

#[derive(Clone, Debug)]
pub struct Walk {
    pub state: WalkState,
    pub steps: Vec<Step>,
}

impl Walk {
    /// Proposals gathered along the way, awaiting verification.
    pub fn provisional(&self) -> impl Iterator<Item = &Proposal> {
        self.steps.iter().flat_map(|s| match s {
            Step::Escalated { proposals, .. } => proposals.as_slice(),
            Step::Followed { .. } => &[],
        })
    }
}

/// A single-line walker. It never forks: where a noul frame could fork,
/// it follows the most probable arrow. `onto-runtime` runs branching walks.
pub struct Walker<'a, J, P> {
    pub cat: &'a Category,
    pub judge: J,
    pub proposer: P,
    /// Minimum System-1 confidence to act alone.
    pub threshold: f32,
}

impl<J: Judge, P: Proposer> Walker<'_, J, P> {
    /// Walks up to `max_steps`, stopping at the first escalation or at a
    /// closed object with no outgoing arrows (a terminal).
    pub fn walk(&mut self, goal: &str, state: Value, from: ObjId, max_steps: usize) -> Walk {
        let mut walk = Walk {
            state: WalkState {
                goal: goal.to_owned(),
                state,
                tokens: BTreeSet::new(),
                at: from,
                path: Path::id(from),
            },
            steps: Vec::new(),
        };
        for _ in 0..max_steps {
            let at = walk.state.at;
            if self.cat.object(at).closure == Closure::Closed && self.cat.out(at).is_empty() {
                break;
            }
            let step = self.step(&walk.state);
            let followed = match step {
                Step::Followed { arrow, .. } => Some(arrow),
                Step::Escalated { .. } => None,
            };
            walk.steps.push(step);
            let Some(arrow) = followed else { break };
            walk.state.tokens = self.cat.arrow(arrow).effect(&walk.state.tokens);
            walk.state
                .path
                .push(self.cat, arrow)
                .expect("frame arrows leave the current object");
            walk.state.at = walk.state.path.dst;
        }
        walk
    }

    pub fn step(&mut self, state: &WalkState) -> Step {
        let frame = candidates(self.cat, state.at, &state.state, &state.tokens);
        let closure = self.cat.object(state.at).closure;
        let answer = (!frame.is_empty()).then(|| self.judge.judge(self.cat, state, &frame));
        let decision = decide(closure, answer.as_ref(), self.threshold, false);
        let reason = match decision {
            Decision::Follow { index, p, .. } => {
                return Step::Followed {
                    arrow: frame[index],
                    p,
                };
            }
            Decision::Fork { branches, .. } => {
                let (index, p) = branches[0];
                return Step::Followed {
                    arrow: frame[index],
                    p,
                };
            }
            Decision::Escalate(reason) => reason,
        };
        Step::Escalated {
            reason,
            proposals: self.proposer.propose(self.cat, state),
        }
    }
}

/// Answers from a fixed script of arrow names, one per step, whatever the
/// primitive: the named arrow gets probability 1. Deterministic, for tests
/// and demos.
pub struct ScriptedJudge {
    script: std::vec::IntoIter<String>,
}

impl ScriptedJudge {
    pub fn new(names: impl IntoIterator<Item = impl Into<String>>) -> Self {
        let script: Vec<String> = names.into_iter().map(Into::into).collect();
        Self {
            script: script.into_iter(),
        }
    }
}

impl Judge for ScriptedJudge {
    fn judge(&mut self, cat: &Category, state: &WalkState, candidates: &[ArrowId]) -> Answer {
        let want = self.script.next();
        let hits: Vec<f32> = candidates
            .iter()
            .map(|a| f32::from(want.as_deref() == Some(cat.arrow(*a).name.as_str())))
            .collect();
        let hit = hits.contains(&1.0);
        match cat.object(state.at).frame.primitive {
            Primitive::Choice => Answer::Choice(Distribution {
                arrows: hits,
                none_of_these: if hit { 0.0 } else { 1.0 },
                confidence: None,
            }),
            Primitive::Noul => Answer::Noul {
                holds: hits,
                fork: None,
            },
            Primitive::Score => Answer::Score {
                confidence: Some(if hit { 1.0 } else { 0.0 }),
                levels: hits,
            },
        }
    }
}

/// Spreads probability evenly. For a choice frame the arrows tie with
/// "none of these", so no arrow wins and every step escalates: useful for
/// exercising the System-2 path.
pub struct UniformJudge;

impl Judge for UniformJudge {
    fn judge(&mut self, cat: &Category, state: &WalkState, candidates: &[ArrowId]) -> Answer {
        let n = candidates.len();
        match cat.object(state.at).frame.primitive {
            Primitive::Choice => {
                let p = 1.0 / (n as f32 + 1.0);
                Answer::Choice(Distribution {
                    arrows: vec![p; n],
                    none_of_these: p,
                    confidence: None,
                })
            }
            Primitive::Noul => Answer::Noul {
                holds: vec![0.5; n],
                fork: None,
            },
            Primitive::Score => Answer::Score {
                levels: vec![1.0 / n as f32; n],
                confidence: None,
            },
        }
    }
}

/// Proposes nothing: escalations are recorded, the walk stops.
pub struct NullProposer;

impl Proposer for NullProposer {
    fn propose(&mut self, _: &Category, _: &WalkState) -> Vec<Proposal> {
        Vec::new()
    }
}

impl<J: Judge + ?Sized> Judge for Box<J> {
    fn judge(&mut self, cat: &Category, state: &WalkState, candidates: &[ArrowId]) -> Answer {
        (**self).judge(cat, state, candidates)
    }
}

impl<P: Proposer + ?Sized> Proposer for Box<P> {
    fn propose(&mut self, cat: &Category, state: &WalkState) -> Vec<Proposal> {
        (**self).propose(cat, state)
    }
}

/// What happened to one candidate arrow at a frame visit.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(tag = "kind", rename_all = "snake_case")
)]
pub enum Disposition {
    /// Followed by this walk.
    Selected,
    /// Pursued as a branch: `branch` is the spawned walk, or `None` when
    /// this walk itself continued along it.
    Forked { branch: Option<u64> },
    /// Held (noul) but not pursued.
    Alternative,
    /// Judged and not taken.
    Rejected,
    /// The frame escalated; the arrow was neither taken nor ruled out.
    Deferred,
    /// Removed by its `require` before any model was asked.
    FilteredByRequire,
    /// Removed by the target's entry contract: tokens the walk would not
    /// hold, or the target's case precondition.
    BlockedByEntry,
    /// Removed because its `attested` precondition is not met by verified,
    /// signed observations.
    Unattested,
}

/// One candidate's judgment, disposition and the reason for it.
#[derive(Clone, Debug, PartialEq)]
pub struct CandidateDisposition {
    pub arrow: ArrowId,
    /// The judge's number for this arrow: choice probability, noul P(holds)
    /// or score level probability. `None` if it was never judged.
    pub judgment: Option<f32>,
    pub disposition: Disposition,
    pub reason: String,
}

/// Explains a frame visit candidate by candidate: every arrow out of `at`,
/// whether `require` let it through, what the judge said, and why the
/// decision treated it as it did. Deterministic: reasons are built from the
/// numbers, not generated.
#[allow(clippy::too_many_arguments)]
pub fn dispose(
    cat: &Category,
    at: ObjId,
    case: &Value,
    tokens: &BTreeSet<String>,
    eligible: &[ArrowId],
    answer: Option<&Answer>,
    decision: &Decision,
    threshold: f32,
    can_fork: bool,
) -> Vec<CandidateDisposition> {
    let primitive = cat.object(at).frame.primitive;
    let judgment = |i: usize| -> Option<f32> {
        match answer? {
            Answer::Choice(d) => d.arrows.get(i).copied(),
            Answer::Noul { holds, .. } => holds.get(i).copied(),
            Answer::Score { levels, .. } => levels.get(i).copied(),
        }
    };
    let confidence = answer.and_then(Answer::confidence);
    let fork_p = match answer {
        Some(Answer::Noul { fork, .. }) => *fork,
        _ => None,
    };
    let name = |a: ArrowId| cat.arrow(a).name.as_str();
    let level = |a: ArrowId| cat.arrow(a).level.unwrap_or_default();
    let gate = |p: f32| match confidence {
        Some(c) => format!("confidence {c:.2} ≥ threshold {threshold}"),
        None => format!("p {p:.2} ≥ threshold {threshold}"),
    };

    let mut out = Vec::new();
    for &a in cat.out(at) {
        if eligible.contains(&a) {
            continue;
        }
        let arrow = cat.arrow(a);
        let (disposition, reason) = match cat.gate(a, case, tokens) {
            Gate::Entry {
                missing,
                require_failed,
            } => {
                let target = cat.object(arrow.dst);
                let mut why = Vec::new();
                if !missing.is_empty() {
                    why.push(format!(
                        "needs {} on entry, which this walk would not hold",
                        missing.join(", ")
                    ));
                }
                if require_failed {
                    let r = target
                        .entry
                        .require
                        .as_ref()
                        .map_or(String::new(), |r| r.to_string());
                    why.push(format!(
                        "requires `{r}` on entry, which this case does not show"
                    ));
                }
                (
                    Disposition::BlockedByEntry,
                    format!("{} {}", target.name, why.join(" and ")),
                )
            }
            Gate::Unattested => {
                let need = arrow
                    .attested
                    .as_ref()
                    .map_or(String::new(), |r| r.to_string());
                let (_, ok, rejected) = crate::attest::attested_view(cat.attesters(), case);
                let mut why = format!("needs attested `{need}`");
                if ok.is_empty() && rejected.is_empty() {
                    why.push_str("; the case carries no observations");
                } else {
                    let valid: Vec<String> = ok
                        .iter()
                        .map(|v| format!("{}: {}", v.attester, v.fields.join(", ")))
                        .collect();
                    if !valid.is_empty() {
                        why.push_str(&format!(
                            "; verified observations do not establish it ({})",
                            valid.join("; ")
                        ));
                    }
                    for r in rejected {
                        why.push_str(&format!("; rejected {}: {}", r.attester, r.reason));
                    }
                }
                (Disposition::Unattested, why)
            }
            _ => {
                let req = arrow
                    .require
                    .as_ref()
                    .map_or(String::new(), |r| r.to_string());
                (
                    Disposition::FilteredByRequire,
                    format!("require `{req}` did not hold for this case"),
                )
            }
        };
        out.push(CandidateDisposition {
            arrow: a,
            judgment: None,
            disposition,
            reason,
        });
    }

    let selected = match decision {
        Decision::Follow { index, .. } => Some(eligible[*index]),
        _ => None,
    };
    for (i, &a) in eligible.iter().enumerate() {
        let p = judgment(i);
        let pv = p.unwrap_or(0.0);
        let (disposition, reason) = match decision {
            Decision::Follow { index, .. } if *index == i => {
                let reason = match primitive {
                    Primitive::Choice => format!("most probable option (p {pv:.2}); {}", gate(pv)),
                    Primitive::Noul => format!("condition holds (p {pv:.2} ≥ {threshold})"),
                    Primitive::Score => format!(
                        "level {} is the most probable (p {pv:.2}); {}",
                        level(a),
                        gate(pv)
                    ),
                };
                (Disposition::Selected, reason)
            }
            Decision::Follow { alternatives, .. } if alternatives.iter().any(|(j, _)| *j == i) => {
                let why = match (can_fork, fork_p) {
                    (true, Some(f)) => {
                        format!("judged a competing reading of the same case (fork p {f:.2} < 0.5)")
                    }
                    _ => "no fork allowed here (branch budget or depth)".to_owned(),
                };
                (
                    Disposition::Alternative,
                    format!("condition also holds (p {pv:.2}), but {why}"),
                )
            }
            Decision::Fork { branches, fork_p } if branches.iter().any(|(j, _)| *j == i) => (
                Disposition::Forked { branch: None },
                format!(
                    "condition holds (p {pv:.2}); an independent aspect of the case (fork p {fork_p:.2})"
                ),
            ),
            Decision::Follow { .. } | Decision::Fork { .. } => {
                let reason = match (primitive, selected) {
                    (Primitive::Choice, Some(s)) => format!("p {pv:.2}, below `{}`", name(s)),
                    (Primitive::Noul, _) => {
                        format!("condition does not hold (p {pv:.2} < {threshold})")
                    }
                    (Primitive::Score, _) => {
                        format!("level {} less probable (p {pv:.2})", level(a))
                    }
                    (Primitive::Choice, None) => format!("p {pv:.2}"),
                };
                (Disposition::Rejected, reason)
            }
            Decision::Escalate(e) => {
                let why = match (e, primitive, answer) {
                    (Escalation::LowConfidence, _, Some(_)) if confidence.is_some() => {
                        format!(
                            "the judge was unsure (confidence {:.2} < {threshold})",
                            confidence.unwrap_or(0.0)
                        )
                    }
                    (Escalation::LowConfidence, Primitive::Noul, _) => {
                        "no condition clearly held or clearly failed".to_owned()
                    }
                    (Escalation::LowConfidence, _, _) => "the judge was unsure".to_owned(),
                    (_, Primitive::Choice, Some(Answer::Choice(d))) => {
                        format!("none_of_these was more likely (p {:.2})", d.none_of_these)
                    }
                    (Escalation::OpenFrame, _, _) => {
                        "nothing fit, and this frame is known to be incomplete".to_owned()
                    }
                    _ => "no condition held".to_owned(),
                };
                (
                    Disposition::Deferred,
                    format!("p {pv:.2}; escalated because {why}"),
                )
            }
        };
        out.push(CandidateDisposition {
            arrow: a,
            judgment: p,
            disposition,
            reason,
        });
    }
    out
}
