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

use crate::category::{ArrowId, Category, Closure, ObjId, Primitive};
use crate::path::Path;

/// What the models see at each step.
#[derive(Clone, Debug)]
pub struct WalkState {
    /// Free-text goal or context for the models.
    pub goal: String,
    /// Structured facts about the case; `require` clauses read it.
    pub state: Value,
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
}

impl Escalation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OpenFrame => "open_frame",
            Self::NoneOfThese => "none_of_these",
            Self::LowConfidence => "low_confidence",
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
pub fn candidates(cat: &Category, at: ObjId, state: &Value) -> Vec<ArrowId> {
    let mut c = cat.eligible(at, state);
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
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Proposal {
    pub arrow: String,
    pub src: String,
    pub dst: String,
    /// What the proposed arrow means: when to follow it.
    pub about: String,
    pub rationale: String,
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
            walk.state
                .path
                .push(self.cat, arrow)
                .expect("frame arrows leave the current object");
            walk.state.at = walk.state.path.dst;
        }
        walk
    }

    pub fn step(&mut self, state: &WalkState) -> Step {
        let frame = candidates(self.cat, state.at, &state.state);
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
