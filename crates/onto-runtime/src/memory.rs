//! Episodic memory: earlier decisions at a frame, shown as precedents.
//!
//! A frame whose `state` declares `memory: similar N` sees up to N earlier
//! decisions made at the same frame. A precedent carries only what the
//! frame is allowed to see now: the earlier state is projected onto the
//! current declaration (goal, the declared case fields, observed), so an
//! `unseen` field can never arrive through memory. Earlier precedents,
//! history, tokens and focus are dropped.
//!
//! Precedents do come from *other cases*: the fields a frame sees about
//! one person are shown when deciding for another. That is what the
//! declaration says; declare memory only where that is acceptable.
//!
//! Similarity is lexical (word overlap of the projected states): cheap,
//! deterministic, and blunt. Nothing about it is a judgment.

use std::collections::BTreeSet;

use onto_core::{CaseView, StateSpec};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Precedent {
    pub frame: String,
    /// The frame record it came from.
    pub record: String,
    pub snapshot: Option<String>,
    /// The case's `id`, if any: a case is never its own precedent. Kept in
    /// the memory file, never shown to a model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub case: Option<String>,
    /// What the frame saw, projected onto its declaration.
    pub saw: Value,
    /// The arrow followed and its probability.
    pub decided: String,
    pub p: f32,
}

/// `state` restricted to what `spec` allows a precedent to carry.
pub fn project(state: &Value, spec: &StateSpec) -> Value {
    let mut out = Map::new();
    if spec.goal
        && let Some(g) = state.get("goal")
    {
        out.insert("goal".into(), g.clone());
    }
    if let Some(a) = state.get("asserted") {
        let kept = match &spec.case {
            CaseView::Hidden => None,
            CaseView::Whole => Some(a.clone()),
            CaseView::Fields(fields) => {
                let mut picked = Value::Object(Map::new());
                for f in fields {
                    if let Some(v) = f.split('.').try_fold(a, |v, k| v.get(k)) {
                        crate::engine::insert_path(&mut picked, f, v.clone());
                    }
                }
                Some(picked)
            }
        };
        if let Some(k) = kept {
            out.insert("asserted".into(), k);
        }
    }
    if spec.observed
        && let Some(o) = state.get("observed")
    {
        out.insert("observed".into(), o.clone());
    }
    Value::Object(out)
}

/// The `n` precedents at `frame` most similar to `now` (a projected
/// state), best first; ties keep the older one first.
pub fn similar<'a>(
    store: &'a [Precedent],
    frame: &str,
    case: Option<&str>,
    now: &Value,
    n: usize,
) -> Vec<&'a Precedent> {
    let here = words(now);
    let mut scored: Vec<(f32, usize, &Precedent)> = store
        .iter()
        .enumerate()
        .filter(|(_, p)| p.frame == frame && (case.is_none() || p.case.as_deref() != case))
        .map(|(i, p)| {
            let there = words(&p.saw);
            let inter = here.intersection(&there).count() as f32;
            let union = here.union(&there).count().max(1) as f32;
            (inter / union, i, p)
        })
        .filter(|(s, ..)| *s > 0.0)
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().take(n).map(|(.., p)| p).collect()
}

fn words(v: &Value) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    collect(v, &mut out);
    out
}

fn collect(v: &Value, out: &mut BTreeSet<String>) {
    match v {
        Value::String(s) => out.extend(
            s.split(|c: char| !c.is_alphanumeric())
                .filter(|w| w.len() > 3)
                .map(str::to_lowercase),
        ),
        Value::Array(xs) => xs.iter().for_each(|x| collect(x, out)),
        Value::Object(m) => m.values().for_each(|x| collect(x, out)),
        other => {
            out.insert(other.to_string());
        }
    }
}
