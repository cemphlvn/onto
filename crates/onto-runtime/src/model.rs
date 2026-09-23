//! Async model interfaces. Requests are owned snapshots of the walk state,
//! so calls can run on any task, concurrently with other walks.
//!
//! The graph stores meaning once (`about`, arrow instructions, levels);
//! each provider's adapter renders a [`FrameRequest`] into its own input
//! format and primitive.

use std::future::Future;
use std::time::Duration;

use onto_core::Primitive;
use onto_core::walk::{Answer, Distribution, Proposal};
use serde::Serialize;
use serde_json::Value;

/// One hop already taken, so judges can reason about the walk so far.
#[derive(Clone, Debug, Serialize)]
pub struct Hop {
    pub from: String,
    pub arrow: String,
    pub to: String,
    pub decided_by: Primitive,
    pub p: f32,
}

#[derive(Clone, Debug, Serialize)]
pub struct Candidate {
    pub arrow: String,
    pub to: String,
    /// When to follow this arrow (text or structured JSON), if declared.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<u32>,
}

/// The one aspect of a case a branch handles after a fork: the arrow that
/// spawned it and that arrow's condition.
#[derive(Clone, Debug, Serialize)]
pub struct Focus {
    pub arrow: String,
    pub to: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<Value>,
}

/// What a System-1 judge sees for one frame.
#[derive(Clone, Debug, Serialize)]
pub struct FrameRequest {
    /// Everything the model may see of the case and the walk, built from
    /// the frame's `state` declaration (`onto_core::state`). Providers
    /// send this and nothing else about the case.
    pub state: Value,
    pub at: String,
    /// Set after a fork: judge only this aspect of the case (phrases the
    /// questions; it is in `state` only when declared).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focus: Option<Focus>,
    pub primitive: Primitive,
    /// The frame's own question, if declared.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<Value>,
    /// Eligible arrows (their `require` held), by level for score frames.
    pub candidates: Vec<Candidate>,
    /// Code allows a fork here (branch budget and depth); only then is the
    /// fork question asked.
    pub can_fork: bool,
    /// The frame declares parallel alternatives: no fork question is asked.
    pub parallel: bool,
}

/// What a System-2 proposer sees: the same frame, plus why System 1 gave up
/// and the objects that already exist (so it can reuse them).
#[derive(Clone, Debug, Serialize)]
pub struct ProposalRequest {
    /// What this frame may see (the same state the judge saw).
    pub state: Value,
    pub at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub about_at: Option<Value>,
    pub path_so_far: String,
    /// Set after a fork: propose only for this aspect of the case.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focus: Option<Focus>,
    pub primitive: Primitive,
    pub frame: Vec<Candidate>,
    pub reason: String,
    pub known_objects: Vec<String>,
    /// Terminal objects (closed, no outgoing arrows): where a walk ends
    /// successfully, each with a few declared arrows that finish into it
    /// (`arrow: From -> To (about)`), showing how this graph ends a case.
    /// A learned branch that never reaches one leaves the case unfinished.
    pub outcomes: Vec<Value>,
    /// Provisional arrows other walks already proposed at this frame. A
    /// proposer should reuse one unchanged when it fits.
    pub pending_here: Vec<Pending>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Pending {
    pub walk: u64,
    pub arrow: String,
    pub target: String,
    pub about: String,
}

/// Call metadata reported alongside every answer, for telemetry.
#[derive(Clone, Copy, Debug, Default)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub attempts: u32,
    /// Questions sent in the request (a noul frame asks several).
    pub questions: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("http: {0}")]
    Http(#[from] reqwest::Error),
    #[error("{status}: {body}")]
    Status { status: u16, body: String },
    #[error("unexpected response: {0}")]
    Decode(String),
    #[error("frame has {0} arrows; a Choice holds at most 254 plus none_of_these")]
    FrameTooWide(usize),
    #[error("{model} does not support {primitive} frames")]
    Unsupported {
        model: String,
        primitive: &'static str,
    },
}

/// System 1: a fast, calibrated judgment of one frame.
pub trait Judge: Send + Sync + 'static {
    fn name(&self) -> String;
    fn judge(
        &self,
        req: FrameRequest,
    ) -> impl Future<Output = Result<(Answer, Usage), ModelError>> + Send;
}

/// System 2: suggests structure a frame is missing.
pub trait Proposer: Send + Sync + 'static {
    fn name(&self) -> String;
    fn propose(
        &self,
        req: ProposalRequest,
    ) -> impl Future<Output = Result<(Vec<Proposal>, Usage), ModelError>> + Send;
}

/// One yes/no question for a [`Critic`]: instructions (text or JSON) and
/// optional descriptions of what yes and no mean.
#[derive(Clone, Debug, Serialize)]
pub struct NoulQuestion {
    pub instructions: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub criteria: Option<Value>,
}

/// Yes/no judgments for the supervisor: P(yes) per question, all asked in
/// one request.
pub trait Critic: Send + Sync + 'static {
    fn name(&self) -> String;
    fn nouls(
        &self,
        state: Value,
        questions: Vec<NoulQuestion>,
    ) -> impl Future<Output = Result<(Vec<f32>, Usage), ModelError>> + Send;
}

/// Offline critic, deterministic, reading the structured instructions the
/// supervisor sends: duplicate when the targets or arrow names match
/// (case- and separator-insensitive); overlap never; a rule is violated
/// when the rule and the proposal share a word longer than five letters.
pub struct MockCritic;

impl Critic for MockCritic {
    fn name(&self) -> String {
        "mock-critic".into()
    }

    async fn nouls(
        &self,
        _: Value,
        questions: Vec<NoulQuestion>,
    ) -> Result<(Vec<f32>, Usage), ModelError> {
        let key = |v: &Value| -> String {
            v.as_str()
                .unwrap_or_default()
                .chars()
                .filter(|c| c.is_alphanumeric())
                .flat_map(char::to_lowercase)
                .collect()
        };
        let words = |v: &Value| -> Vec<String> {
            v.to_string()
                .split(|c: char| !c.is_alphanumeric())
                .filter(|w| w.len() > 5)
                .map(str::to_lowercase)
                .collect()
        };
        let ps = questions
            .iter()
            .map(|q| {
                let i = &q.instructions;
                let (p, e) = (&i["proposal"], &i["existing"]);
                match i["check"].as_str() {
                    Some("duplicate") => {
                        let same =
                            key(&p["to"]) == key(&e["to"]) || key(&p["arrow"]) == key(&e["arrow"]);
                        if same { 0.9 } else { 0.1 }
                    }
                    Some("rule") => {
                        let rule = words(&i["rule"]);
                        if words(p).iter().any(|w| rule.contains(w)) {
                            0.9
                        } else {
                            0.1
                        }
                    }
                    _ => 0.1,
                }
            })
            .collect();
        Ok((
            ps,
            Usage {
                attempts: 1,
                questions: questions.len() as u32,
                ..Usage::default()
            },
        ))
    }
}

/// Offline judge: an arrow applies when its name, target or instruction
/// text appears in the goal. Several arrows applying and the goal saying
/// "and" means independent aspects (fork). After a fixed latency.
pub struct MockJudge {
    pub latency: Duration,
}

impl MockJudge {
    fn hit(goal: &str, c: &Candidate) -> bool {
        let words = [c.arrow.as_str(), c.to.as_str()];
        words.iter().any(|w| goal.contains(&w.to_lowercase()))
            || c.instructions
                .as_ref()
                .and_then(Value::as_str)
                .is_some_and(|t| {
                    t.split(|ch: char| !ch.is_alphanumeric())
                        .any(|w| w.len() > 3 && goal.contains(&w.to_lowercase()))
                })
    }
}

/// The mock judge critiques like [`MockCritic`] (open world reviews with
/// the judge).
impl Critic for MockJudge {
    fn name(&self) -> String {
        MockCritic.name()
    }

    async fn nouls(
        &self,
        state: Value,
        questions: Vec<NoulQuestion>,
    ) -> Result<(Vec<f32>, Usage), ModelError> {
        MockCritic.nouls(state, questions).await
    }
}

impl Judge for MockJudge {
    fn name(&self) -> String {
        format!("mock({}ms)", self.latency.as_millis())
    }

    async fn judge(&self, req: FrameRequest) -> Result<(Answer, Usage), ModelError> {
        tokio::time::sleep(self.latency).await;
        let goal = req.state["goal"]
            .as_str()
            .unwrap_or_default()
            .to_lowercase();
        let hits: Vec<bool> = req.candidates.iter().map(|c| Self::hit(&goal, c)).collect();
        let first = hits.iter().position(|h| *h);
        let answer = match req.primitive {
            Primitive::Choice => {
                let mut arrows = vec![0.0; hits.len()];
                if let Some(i) = first {
                    arrows[i] = 0.95;
                }
                Answer::Choice(Distribution {
                    arrows,
                    none_of_these: if first.is_some() { 0.05 } else { 1.0 },
                    confidence: Some(0.9),
                })
            }
            Primitive::Noul => Answer::Noul {
                holds: hits.iter().map(|h| if *h { 0.95 } else { 0.05 }).collect(),
                fork: req
                    .can_fork
                    .then(|| if goal.contains(" and ") { 0.9 } else { 0.1 }),
            },
            Primitive::Split => Answer::Noul {
                holds: vec![1.0; hits.len()],
                fork: None,
            },
            Primitive::Score => {
                let mut levels = vec![0.0; hits.len()];
                levels[first.unwrap_or(0)] = 1.0;
                Answer::Score {
                    levels,
                    confidence: Some(if first.is_some() { 0.9 } else { 0.2 }),
                }
            }
        };
        let questions = match req.primitive {
            Primitive::Noul => hits.len() as u32 + u32::from(req.can_fork),
            _ => 1,
        };
        Ok((
            answer,
            Usage {
                attempts: 1,
                questions,
                ..Usage::default()
            },
        ))
    }
}

/// Offline proposer: reuses a pending proposal whose target the goal
/// mentions; otherwise suggests one arrow to a new object named after the
/// goal's last word. After a fixed latency.
pub struct MockProposer {
    pub latency: Duration,
}

impl Proposer for MockProposer {
    fn name(&self) -> String {
        format!("mock({}ms)", self.latency.as_millis())
    }

    async fn propose(&self, req: ProposalRequest) -> Result<(Vec<Proposal>, Usage), ModelError> {
        tokio::time::sleep(self.latency).await;
        let goal = req.state["goal"]
            .as_str()
            .unwrap_or_default()
            .to_lowercase();
        if let Some(p) = req
            .pending_here
            .iter()
            .find(|p| goal.contains(&p.target.to_lowercase()))
        {
            let reused = Proposal {
                arrow: p.arrow.clone(),
                src: req.at.clone(),
                dst: p.target.clone(),
                about: p.about.clone(),
                rationale: format!("same kind of case as walk {}'s proposal", p.walk),
                ..Default::default()
            };
            return Ok((
                vec![reused],
                Usage {
                    attempts: 1,
                    questions: 1,
                    ..Usage::default()
                },
            ));
        }
        let word: String = req
            .focus
            .as_ref()
            .map_or(req.state["goal"].as_str().unwrap_or("Other"), |f| {
                f.arrow.as_str()
            })
            .split_whitespace()
            .last()
            .unwrap_or("Other")
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect();
        let mut dst = word.clone();
        if let Some(first) = dst.get_mut(0..1) {
            first.make_ascii_uppercase();
        }
        let proposal = Proposal {
            arrow: format!("to_{}", word.to_lowercase()),
            src: req.at,
            dst,
            about: format!("cases about {word}"),
            rationale: format!("goal mentions `{word}`, which no arrow here covers"),
            ..Default::default()
        };
        Ok((
            vec![proposal],
            Usage {
                attempts: 1,
                questions: 1,
                ..Usage::default()
            },
        ))
    }
}
