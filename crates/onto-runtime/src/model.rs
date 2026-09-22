//! Async model interfaces. Requests are owned snapshots of the walk state so
//! calls can run on any task, concurrently with other walks.

use std::future::Future;
use std::time::Duration;

use onto_core::walk::{Distribution, Proposal};
use serde::Serialize;

/// What a System-1 chooser sees: the goal, where the walk stands, and the
/// closed frame to choose from (`none_of_these` is added by the chooser).
#[derive(Clone, Debug, Serialize)]
pub struct ChoiceRequest {
    pub goal: String,
    pub at: String,
    pub path_so_far: String,
    pub frame: Vec<Candidate>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Candidate {
    pub arrow: String,
    pub to: String,
}

/// What a System-2 proposer sees: the same, plus why System 1 gave up and
/// the objects that already exist (so it can reuse them).
#[derive(Clone, Debug, Serialize)]
pub struct ProposalRequest {
    pub goal: String,
    pub at: String,
    pub path_so_far: String,
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
}

pub trait Chooser: Send + Sync + 'static {
    fn name(&self) -> String;
    fn choose(
        &self,
        req: ChoiceRequest,
    ) -> impl Future<Output = Result<(Distribution, Usage), ModelError>> + Send;
}

pub trait Proposer: Send + Sync + 'static {
    fn name(&self) -> String;
    fn propose(
        &self,
        req: ProposalRequest,
    ) -> impl Future<Output = Result<(Vec<Proposal>, Usage), ModelError>> + Send;
}

/// Offline chooser: picks the arrow whose name or target appears in the
/// goal, after a fixed latency. Lets the loop run without API keys.
pub struct MockChooser {
    pub latency: Duration,
}

impl Chooser for MockChooser {
    fn name(&self) -> String {
        format!("mock({}ms)", self.latency.as_millis())
    }

    async fn choose(&self, req: ChoiceRequest) -> Result<(Distribution, Usage), ModelError> {
        tokio::time::sleep(self.latency).await;
        let goal = req.goal.to_lowercase();
        let hit = req.frame.iter().position(|c| {
            goal.contains(&c.arrow.to_lowercase()) || goal.contains(&c.to.to_lowercase())
        });
        let mut arrows = vec![0.0; req.frame.len()];
        if let Some(i) = hit {
            arrows[i] = 0.95;
        }
        let d = Distribution {
            arrows,
            none_of_these: if hit.is_some() { 0.05 } else { 1.0 },
            confidence: Some(0.9),
        };
        Ok((
            d,
            Usage {
                attempts: 1,
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
            rationale: format!("goal mentions `{word}`, which no arrow here covers"),
        };
        Ok((
            vec![proposal],
            Usage {
                attempts: 1,
                ..Usage::default()
            },
        ))
    }
}
