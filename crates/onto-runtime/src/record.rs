//! Disposition records: the provenance artifact.
//!
//! One [`FrameRecord`] per frame visit, holding every candidate arrow with
//! the judge's number, its disposition and a reason built from the numbers.
//! Records chain through `after` (the previous visit in the same walk, or
//! the fork a branch came from), so a run's records form a DAG: the
//! disposition graph. Written as JSON lines by `onto run --dispositions`,
//! not reconstructed from telemetry.

use onto_core::walk::{CandidateDisposition, Disposition, Escalation, Proposal};
use onto_core::{Category, Primitive};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct FrameRecord {
    /// `w{walk}.{n}`: the n-th frame this walk visited.
    pub id: String,
    /// The visit this one follows causally.
    pub after: Option<String>,
    /// SHA-256 of the category source this visit was made against.
    pub snapshot: Option<String>,
    pub walk: u64,
    /// The aspect this walk handles, if it is (or continued) a fork branch.
    pub focus: Option<String>,
    /// Capability tokens the walk held at this visit.
    pub tokens: Vec<String>,
    pub at: String,
    pub primitive: Primitive,
    /// `closed` (the frame claims to be complete) or `open`.
    pub closure: &'static str,
    pub claim: ClaimRecord,
    /// `None` when no model was asked (nothing eligible, or the call failed).
    pub judge: Option<JudgeRecord>,
    /// The state the model was shown at this visit (judge or proposer),
    /// exactly as sent: what it saw of the case, per the `state` policy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seen: Option<serde_json::Value>,
    pub candidates: Vec<CandidateRecord>,
    pub outcome: Outcome,
    /// Provisional System-2 proposals made at this visit, if it escalated.
    pub proposals: Vec<Proposal>,
    /// A `grouped by` frame's coarse step.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grouped: Option<crate::lens::GroupRecord>,
    /// Open world: proposals the supervisor refused here, with the reason.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub refused: Vec<(String, String)>,
    /// Assured admission: proposals held for a person, with the undecided checks.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub held: Vec<(String, String)>,
    /// Attestations that opened the arrow this visit followed: who
    /// observed what, and when (empty: the step rests on judgment alone).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub attested: Vec<String>,
    /// For a join: the other branches' last records, merged into this one
    /// (the disposition graph's merge nodes).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub merged_from: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ClaimRecord {
    pub mode: crate::frames::Mode,
    pub wait_ms: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct JudgeRecord {
    pub model: String,
    pub latency_ms: f64,
    pub questions: u32,
    pub confidence: Option<f32>,
    /// Choice frames only.
    pub none_of_these: Option<f32>,
    /// Noul frames where a fork was allowed.
    pub fork_p: Option<f32>,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct CandidateRecord {
    pub arrow: String,
    pub to: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub require: Option<String>,
    pub judgment: Option<f32>,
    pub disposition: Disposition,
    pub reason: String,
    /// Learned structure (open world), not declared policy.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub learned: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Outcome {
    Followed {
        arrow: String,
        to: String,
    },
    Forked {
        continued: String,
        spawned: Vec<u64>,
    },
    Escalated {
        reason: Escalation,
    },
    /// Open world: System 2's `learned` arrows were admitted into the
    /// live graph and the walk re-judged this frame with them.
    Expanded {
        reason: Escalation,
        learned: Vec<String>,
        /// `proposer`, or `transport F` (empty fibers of functor F).
        source: String,
    },
    Failed {
        error: String,
    },
    /// Arrived at a join object: `role` is `continued`, `ended` or
    /// `escalated`; `with` are the sibling walks folded in.
    Joined {
        policy: String,
        role: String,
        with: Vec<u64>,
        detail: String,
    },
}

pub(crate) fn candidates(cat: &Category, ds: Vec<CandidateDisposition>) -> Vec<CandidateRecord> {
    ds.into_iter()
        .map(|d| {
            let a = cat.arrow(d.arrow);
            CandidateRecord {
                arrow: a.name.clone(),
                to: cat.object(a.dst).name.clone(),
                require: a.require.as_ref().map(ToString::to_string),
                judgment: d.judgment,
                disposition: d.disposition,
                reason: d.reason,
                learned: a.learned,
            }
        })
        .collect()
}
