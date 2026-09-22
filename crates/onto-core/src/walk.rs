//! Two-tier walks.
//!
//! At each object the walker reads its decision frame (outgoing arrows).
//! If the frame is `Closed` (a verified MECE enumeration), a System-1
//! [`Chooser`] picks among the arrows plus an explicit "none of these".
//! If the frame is `Open`, the chooser answers "none of these", or its top
//! probability is below the threshold, the step escalates to a System-2
//! [`Proposer`], which may suggest new structure. Proposals are recorded as
//! provisional; they never enter the category without verification.

use crate::category::{ArrowId, Category, Closure, ObjId};
use crate::path::Path;

/// What the models see at each step.
#[derive(Clone, Debug)]
pub struct WalkState {
    /// Free-text goal or context for the models.
    pub goal: String,
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

/// System 1: a fast, calibrated choice over a closed frame.
pub trait Chooser {
    fn choose(&mut self, cat: &Category, state: &WalkState, candidates: &[ArrowId])
    -> Distribution;
}

/// A new arrow suggested by the System-2 proposer. The target may be an
/// existing object or a new one.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Proposal {
    pub arrow: String,
    pub src: String,
    pub dst: String,
    pub rationale: String,
}

/// System 2: generates structure the frame is missing.
pub trait Proposer {
    fn propose(&mut self, cat: &Category, state: &WalkState) -> Vec<Proposal>;
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

pub struct Walker<'a, C, P> {
    pub cat: &'a Category,
    pub chooser: C,
    pub proposer: P,
    /// Minimum top probability for System 1 to act alone.
    pub threshold: f32,
}

impl<C: Chooser, P: Proposer> Walker<'_, C, P> {
    /// Walks up to `max_steps`, stopping at the first escalation or at a
    /// closed object with no outgoing arrows (a terminal).
    pub fn walk(&mut self, goal: &str, from: ObjId, max_steps: usize) -> Walk {
        let mut walk = Walk {
            state: WalkState {
                goal: goal.to_owned(),
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
        let frame = self.cat.out(state.at);
        let closure = self.cat.object(state.at).closure;
        let d = (closure == Closure::Closed).then(|| self.chooser.choose(self.cat, state, frame));
        let reason = match decide(closure, d.as_ref(), self.threshold) {
            Decision::Follow { index, p } => {
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Decision {
    /// Follow the frame arrow at `index`; `p` is its probability.
    Follow {
        index: usize,
        p: f32,
    },
    Escalate(Escalation),
}

/// The System-1 gate, shared by every walker. `d` is the chooser's answer
/// for a closed frame (`None` for an open one). The model's confidence gates
/// when present, the top probability otherwise.
pub fn decide(closure: Closure, d: Option<&Distribution>, threshold: f32) -> Decision {
    let Some(d) = d.filter(|_| closure == Closure::Closed) else {
        return Decision::Escalate(Escalation::OpenFrame);
    };
    match d.top() {
        Some((index, p)) if d.confidence.unwrap_or(p) >= threshold => Decision::Follow { index, p },
        Some(_) => Decision::Escalate(Escalation::LowConfidence),
        None => Decision::Escalate(Escalation::NoneOfThese),
    }
}

/// Picks arrows in a fixed order by name; answers "none of these" when the
/// next scripted name is not in the frame. Deterministic, for tests and demos.
pub struct ScriptedChooser {
    script: std::vec::IntoIter<String>,
}

impl ScriptedChooser {
    pub fn new(names: impl IntoIterator<Item = impl Into<String>>) -> Self {
        let script: Vec<String> = names.into_iter().map(Into::into).collect();
        Self {
            script: script.into_iter(),
        }
    }
}

impl Chooser for ScriptedChooser {
    fn choose(&mut self, cat: &Category, _: &WalkState, candidates: &[ArrowId]) -> Distribution {
        let want = self.script.next();
        let arrows: Vec<f32> = candidates
            .iter()
            .map(|a| f32::from(want.as_deref() == Some(cat.arrow(*a).name.as_str())))
            .collect();
        let none_of_these = if arrows.contains(&1.0) { 0.0 } else { 1.0 };
        Distribution {
            arrows,
            none_of_these,
            confidence: None,
        }
    }
}

/// Spreads probability evenly over the frame and "none of these". The tie
/// means no arrow wins, so every step escalates; useful for exercising the
/// System-2 path.
pub struct UniformChooser;

impl Chooser for UniformChooser {
    fn choose(&mut self, _: &Category, _: &WalkState, candidates: &[ArrowId]) -> Distribution {
        let p = 1.0 / (candidates.len() as f32 + 1.0);
        Distribution {
            arrows: vec![p; candidates.len()],
            none_of_these: p,
            confidence: None,
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

impl<C: Chooser + ?Sized> Chooser for Box<C> {
    fn choose(
        &mut self,
        cat: &Category,
        state: &WalkState,
        candidates: &[ArrowId],
    ) -> Distribution {
        (**self).choose(cat, state, candidates)
    }
}

impl<P: Proposer + ?Sized> Proposer for Box<P> {
    fn propose(&mut self, cat: &Category, state: &WalkState) -> Vec<Proposal> {
        (**self).propose(cat, state)
    }
}
