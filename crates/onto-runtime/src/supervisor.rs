//! The supervisor: reviews provisional proposals before anything may be
//! promoted into the graph.
//!
//! Structural checks come first and are proofs (`onto_core::supervise`):
//! a malformed proposal or a broken `via`/`never` invariant is rejected
//! with a reason and, for invariants, a counter-path; no model is asked.
//! Then every semantic check for the proposal goes to the critic in one
//! request: each `rule` invariant, and, against every sibling arrow
//! (existing, or proposed earlier in the batch), a duplicate check and, in
//! choice frames, an overlap check (overlap would break the frame's MECE
//! claim). Model checks fail at P ≥ 0.7, pass at P ≤ 0.3, and are unknown
//! between: the supervisor never guesses.

use onto_core::supervise::{self, Admission, Check, Outcome};
use onto_core::walk::Proposal;
use onto_core::{Category, Invariant, Primitive};
use serde::Serialize;
use serde_json::{Value, json};

use crate::model::{Critic, NoulQuestion};

pub const FAIL_AT: f32 = 0.7;
pub const PASS_AT: f32 = 0.3;

/// The supervisor's verdict on one proposal.
#[derive(Clone, Debug, Serialize)]
pub struct Review {
    pub id: String,
    /// SHA-256 of the category source the proposal was reviewed against.
    /// Promotion requires the file to still be this snapshot.
    pub snapshot: Option<String>,
    /// Frame records the proposal came from (identical proposals merged).
    pub sources: Vec<String>,
    pub proposal: Proposal,
    pub checks: Vec<Check>,
    pub admission: Admission,
    /// The model that judged the semantic checks, if any were asked.
    pub critic: Option<String>,
}

/// A semantic check waiting for the critic's number.
struct Pending {
    check: &'static str,
    subject: String,
    question: NoulQuestion,
}

pub async fn review<C: Critic>(
    cat: &Category,
    id: String,
    sources: Vec<String>,
    p: &Proposal,
    earlier: &[Proposal],
    critic: &C,
) -> Review {
    let (mut checks, extended) = supervise::structural(cat, p);
    let done = |checks: Vec<Check>, critic| Review {
        id: id.clone(),
        snapshot: cat.snapshot().map(str::to_owned),
        sources: sources.clone(),
        proposal: p.clone(),
        admission: supervise::admission(&checks),
        checks,
        critic,
    };
    if extended.is_none() || supervise::admission(&checks) == Admission::Reject {
        // Proved unacceptable: no need to ask a model.
        return done(checks, None);
    }

    let proposed = json!({"arrow": p.arrow, "from": p.src, "to": p.dst, "about": p.about});
    let mut pending = Vec::new();
    for inv in cat.invariants() {
        if let Invariant::Rule(rule) = inv {
            pending.push(Pending {
                check: "rule",
                subject: inv.to_string(),
                question: NoulQuestion {
                    instructions: json!({
                        "check": "rule",
                        "rule": rule,
                        "proposal": proposed,
                        "question": "Would adding the arrow `proposal` to the graph violate `rule`?",
                    }),
                    criteria: Some(json!({
                        "true": "adding it violates the rule",
                        "false": "adding it is consistent with the rule",
                    })),
                },
            });
        }
    }

    let src = cat
        .object_id(&p.src)
        .expect("well-formed proposals have a known source");
    let choice = cat.object(src).frame.primitive == Primitive::Choice;
    let mut siblings: Vec<Value> = cat
        .out(src)
        .iter()
        .map(|a| {
            let a = cat.arrow(*a);
            json!({"arrow": a.name, "to": cat.object(a.dst).name, "about": a.instructions, "status": "committed"})
        })
        .collect();
    siblings.extend(earlier.iter().filter(|e| e.src == p.src).map(
        |e| json!({"arrow": e.arrow, "to": e.dst, "about": e.about, "status": "proposed earlier"}),
    ));
    for s in siblings {
        let subject = format!(
            "{} -> {} ({})",
            s["arrow"].as_str().unwrap_or("?"),
            s["to"].as_str().unwrap_or("?"),
            s["status"].as_str().unwrap_or("?")
        );
        pending.push(Pending {
            check: "duplicate",
            subject: subject.clone(),
            question: NoulQuestion {
                instructions: json!({
                    "check": "duplicate",
                    "proposal": proposed,
                    "existing": s,
                    "question": "Do `proposal` and `existing` describe the same kind of case, so that one of them is redundant?",
                }),
                criteria: None,
            },
        });
        if choice {
            pending.push(Pending {
                check: "overlap",
                subject,
                question: NoulQuestion {
                    instructions: json!({
                        "check": "overlap",
                        "proposal": proposed,
                        "existing": s,
                        "question": "Could a single case fit both `proposal` and `existing`, making the choice between them ambiguous?",
                    }),
                    criteria: None,
                },
            });
        }
    }
    if pending.is_empty() {
        return done(checks, None);
    }

    let state = json!({
        "graph": cat.name(),
        "at": p.src,
        "about_at": cat.object(src).about,
    });
    let questions = pending.iter().map(|q| q.question.clone()).collect();
    match critic.nouls(state, questions).await {
        Ok((ps, _)) => {
            for (q, p) in pending.into_iter().zip(ps) {
                let (outcome, verdict) = if p >= FAIL_AT {
                    (Outcome::Fail, "yes")
                } else if p <= PASS_AT {
                    (Outcome::Pass, "no")
                } else {
                    (Outcome::Unknown, "unsure")
                };
                let reason = match q.check {
                    "rule" => format!("violates the rule? {verdict} (p {p:.2})"),
                    "duplicate" => {
                        format!("same kind of case as this sibling? {verdict} (p {p:.2})")
                    }
                    _ => format!("could one case fit both? {verdict} (p {p:.2})"),
                };
                checks.push(Check {
                    check: q.check.into(),
                    subject: q.subject,
                    outcome,
                    reason,
                    p: Some(p),
                    witness: None,
                });
            }
            done(checks, Some(critic.name()))
        }
        Err(e) => {
            checks.push(Check {
                check: "critic".into(),
                subject: critic.name(),
                outcome: Outcome::Unknown,
                reason: format!("semantic checks not run: {e}"),
                p: None,
                witness: None,
            });
            done(checks, None)
        }
    }
}
