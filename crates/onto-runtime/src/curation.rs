//! Gap signals: stops that new structure could address but not on the
//! case's path (a sealed frame, a closed-world run), collected for
//! curation (`docs/08-call-economy.md`).
//!
//! The case never waits for them. A signal carries what the frame was
//! allowed to show a model (`seen`, already projected by the `state`
//! policy), never the raw case, so curation sees no more than a judge did.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GapSignal {
    /// Snapshot, frame, kind and missing distinction: the same gap has the
    /// same key.
    pub key: String,
    pub kind: String,
    pub frame: String,
    pub reason: String,
    pub missing: String,
    pub record: String,
    pub walk: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub case: Option<String>,
    pub snapshot: Option<String>,
    /// What the frame's models may see of the case.
    pub seen: Value,
    /// The frame's current options (`arrow → target: about`).
    pub options: Vec<String>,
}

/// Signals grouped by gap: one group, one proposal.
#[derive(Clone, Debug, Serialize)]
pub struct GapGroup {
    pub key: String,
    pub kind: String,
    pub frame: String,
    pub missing: String,
    pub signals: Vec<GapSignal>,
}

/// Groups signals by key, in order of first appearance.
pub fn group(signals: Vec<GapSignal>) -> Vec<GapGroup> {
    let mut out: Vec<GapGroup> = Vec::new();
    for s in signals {
        match out.iter_mut().find(|g| g.key == s.key) {
            Some(g) => g.signals.push(s),
            None => out.push(GapGroup {
                key: s.key.clone(),
                kind: s.kind.clone(),
                frame: s.frame.clone(),
                missing: s.missing.clone(),
                signals: vec![s],
            }),
        }
    }
    out
}

/// The proposal request for a group: the frame, its options, and a few
/// representative cases as their frame was allowed to show them.
pub fn request(
    cat: &onto_core::Category,
    g: &GapGroup,
    examples: usize,
) -> Result<crate::model::ProposalRequest, onto_core::Error> {
    use onto_core::{ArrowId, Closure, ObjId};
    let at = cat.object_id(&g.frame)?;
    let object = cat.object(at);
    let mut cases: Vec<Option<String>> = g.signals.iter().map(|s| s.case.clone()).collect();
    cases.sort();
    cases.dedup();
    let state = serde_json::json!({
        "gap": g.missing,
        "kind": g.kind,
        "seen_in_cases": cases.len(),
        "examples": g.signals.iter().take(examples).map(|s| &s.seen).collect::<Vec<_>>(),
        "note": "representative cases, as this frame's state policy allowed a model to see them",
    });
    let outcomes = (0..cat.objects().len() as u32)
        .map(ObjId)
        .filter(|o| cat.object(*o).closure == Closure::Closed && cat.out(*o).is_empty())
        .map(|o| {
            let reached_by: Vec<String> = (0..cat.arrows().len() as u32)
                .map(ArrowId)
                .map(|a| cat.arrow(a))
                .filter(|a| a.dst == o)
                .take(6)
                .map(|a| {
                    format!(
                        "{}: {} -> {}",
                        a.name,
                        cat.object(a.src).name,
                        cat.object(o).name
                    )
                })
                .collect();
            serde_json::json!({"object": cat.object(o).name, "reached_by": reached_by})
        })
        .collect();
    Ok(crate::model::ProposalRequest {
        state,
        at: object.name.clone(),
        about_at: object.about.clone(),
        path_so_far: String::new(),
        focus: None,
        primitive: object.frame.primitive,
        shape: cat.proposal_shape(at),
        frame: cat
            .out(at)
            .iter()
            .map(|a| crate::model::Candidate::of(cat, *a))
            .collect(),
        reason: format!(
            "{}: {} (seen in {} case(s)); for a person to review, not for one case",
            g.kind,
            g.missing,
            cases.len()
        ),
        known_objects: cat.objects().iter().map(|o| o.name.clone()).collect(),
        // A person may extend sealed frames; nothing is hidden from review.
        sealed: Vec::new(),
        outcomes,
        pending_here: Vec::new(),
    })
}
