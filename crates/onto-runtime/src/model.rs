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

/// What a System-1 judge sees for one frame.
#[derive(Clone, Debug, Serialize)]
pub struct FrameRequest {
    pub goal: String,
    /// Structured facts about the case (from the job), possibly empty.
    pub case: Value,
    pub at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub about_at: Option<Value>,
    pub path_so_far: String,
    pub hops: Vec<Hop>,
    pub primitive: Primitive,
    /// The frame's own question, if declared.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<Value>,
    /// Eligible arrows (their `require` held), by level for score frames.
    pub candidates: Vec<Candidate>,
    /// Code allows a fork here (branch budget and depth); only then is the
    /// fork question asked.
    pub can_fork: bool,
}

/// What a System-2 proposer sees: the same frame, plus why System 1 gave up
/// and the objects that already exist (so it can reuse them).
#[derive(Clone, Debug, Serialize)]
pub struct ProposalRequest {
    pub goal: String,
    pub case: Value,
    pub at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub about_at: Option<Value>,
    pub path_so_far: String,
    pub primitive: Primitive,
    pub frame: Vec<Candidate>,
    pub reason: String,
    pub known_objects: Vec<String>,
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

impl Judge for MockJudge {
    fn name(&self) -> String {
        format!("mock({}ms)", self.latency.as_millis())
    }

    async fn judge(&self, req: FrameRequest) -> Result<(Answer, Usage), ModelError> {
        tokio::time::sleep(self.latency).await;
        let goal = req.goal.to_lowercase();
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

/// Offline proposer: suggests one arrow to a new object named after the
/// goal's last word, after a fixed latency.
pub struct MockProposer {
    pub latency: Duration,
}

impl Proposer for MockProposer {
    fn name(&self) -> String {
        format!("mock({}ms)", self.latency.as_millis())
    }

    async fn propose(&self, req: ProposalRequest) -> Result<(Vec<Proposal>, Usage), ModelError> {
        tokio::time::sleep(self.latency).await;
        let word: String = req
            .goal
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
