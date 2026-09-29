//! The model contract: the async interfaces every model provider
//! implements (remote clients in `onto-remote`, local judges, bindings'
//! callbacks), with no engine and no network.
//!
//! Requests are owned snapshots of the walk state, so calls can run on
//! any task, concurrently with other walks. The graph stores meaning once
//! (`about`, arrow instructions, levels); each provider's adapter renders
//! a [`FrameRequest`] into its own input format and primitive.

use std::future::Future;

use onto_core::walk::{Answer, Proposal};
use onto_core::{Primitive, ProposalShape};
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

impl Candidate {
    /// How the arrow reads as an option, condition or level: its
    /// instructions when declared (text gets the target appended; JSON is
    /// wrapped with it), else a sentence built from its name. Every
    /// provider renders arrows this way.
    pub fn describe(&self) -> Value {
        match &self.instructions {
            Some(Value::String(t)) => Value::String(format!("{t} (leads to {})", self.to)),
            Some(structured) => serde_json::json!({"leads_to": self.to, "description": structured}),
            None => Value::String(format!("follow `{}` to {}", self.arrow, self.to)),
        }
    }

    pub fn of(cat: &onto_core::Category, arrow: onto_core::ArrowId) -> Self {
        let a = cat.arrow(arrow);
        Self {
            arrow: a.name.clone(),
            to: cat.object(a.dst).name.clone(),
            instructions: a.instructions.clone(),
            level: a.level,
        }
    }
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
    /// What the frame asks a proposer for (D74). Omitted when `actions`,
    /// so routing requests are unchanged.
    #[serde(skip_serializing_if = "is_actions")]
    pub shape: ProposalShape,
    pub frame: Vec<Candidate>,
    pub reason: String,
    pub known_objects: Vec<String>,
    /// Objects the open world may not extend: never a target of a new
    /// arrow (it would be refused). Only declared arrows reach them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sealed: Vec<String>,
    /// Terminal objects (closed, no outgoing arrows): where a walk ends
    /// successfully, each with a few declared arrows that finish into it
    /// (`arrow: From -> To (about)`), showing how this graph ends a case.
    /// A learned branch that never reaches one leaves the case unfinished.
    pub outcomes: Vec<Value>,
    /// Provisional arrows other walks already proposed at this frame. A
    /// proposer should reuse one unchanged when it fits.
    pub pending_here: Vec<Pending>,
}

fn is_actions(shape: &ProposalShape) -> bool {
    *shape == ProposalShape::Actions
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
    /// Transport failure (connection, TLS, body); the provider's own
    /// error, rendered.
    #[error("http: {0}")]
    Http(String),
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
