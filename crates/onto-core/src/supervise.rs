//! The supervisor's structural half: checks a provisional proposal can be
//! proved against, without any model.
//!
//! Each check passes, fails, or is unknown, with a reason; a failed
//! invariant carries a counter-path as proof. Model-judged checks (rules,
//! duplicates, overlap) live in `onto-runtime`; [`admission`] combines both.

use crate::category::{Category, Invariant};
use crate::walk::Proposal;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "snake_case")
)]
pub enum Outcome {
    Pass,
    Fail,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "snake_case")
)]
pub enum Admission {
    /// Every check passed: a person may promote it.
    Admit,
    /// A check failed: it cannot be promoted.
    Reject,
    /// Nothing failed, but something could not be decided: a person must
    /// look before promoting.
    Unknown,
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Check {
    /// `well_formed`, `closure`, `contract`, `invariant`, `rule`,
    /// `duplicate`, `overlap`.
    pub check: String,
    /// What was checked against (an invariant, an existing arrow, …).
    pub subject: String,
    pub outcome: Outcome,
    pub reason: String,
    /// The model's probability, for judged checks.
    pub p: Option<f32>,
    /// A counter-path proving a failed invariant.
    pub witness: Option<String>,
}

/// Reject if anything failed; unknown if anything was undecided; else admit.
pub fn admission(checks: &[Check]) -> Admission {
    if checks.iter().any(|c| c.outcome == Outcome::Fail) {
        Admission::Reject
    } else if checks.iter().any(|c| c.outcome == Outcome::Unknown) {
        Admission::Unknown
    } else {
        Admission::Admit
    }
}

/// Well-formedness and every structural invariant, on the graph the
/// proposal would produce. Returns that graph when the proposal is
/// well-formed, so callers can run further checks on it.
pub fn structural(cat: &Category, p: &Proposal) -> (Vec<Check>, Option<Category>) {
    let label = format!("{}: {} -> {}", p.arrow, p.src, p.dst);
    let about = (!p.about.is_empty()).then(|| serde_json::Value::String(p.about.clone()));
    let mut checks = Vec::new();
    let extended = match cat.extend(&p.arrow, &p.src, &p.dst, about.clone()) {
        Ok(c) => Some(c),
        Err(e) => {
            checks.push(Check {
                check: "well_formed".into(),
                subject: label.clone(),
                outcome: Outcome::Fail,
                reason: e.to_string(),
                p: None,
                witness: None,
            });
            None
        }
    };
    // A reused name alone should not hide what the arrow would do: judge
    // the invariants on the graph with the arrow under a fresh name.
    let probe = match &extended {
        Some(c) => Some(c.clone()),
        None if cat.arrow_id(&p.arrow).is_ok() => cat
            .extend(&format!("{}__proposed", p.arrow), &p.src, &p.dst, about)
            .ok(),
        None => None,
    };
    let Some(graph) = probe else {
        return (checks, None);
    };
    // A closed frame claims its arrows are all the cases. A proposal there
    // challenges that claim; only a person can revise it.
    if let Ok(src) = cat.object_id(&p.src)
        && cat.object(src).closure == crate::category::Closure::Closed
    {
        checks.push(Check {
            check: "closure".into(),
            subject: format!("{} (closed)", p.src),
            outcome: Outcome::Unknown,
            reason: format!(
                "closure challenge: {} claims its arrows are complete; admitting a novel arrow revises that claim, which needs a person",
                p.src
            ),
            p: None,
            witness: None,
        });
    }
    // Entry contracts: can any walk ever take the new arrow? (`extend`
    // adds the proposal as the last arrow.)
    let new = crate::category::ArrowId(graph.arrows().len() as u32 - 1);
    if let Ok(src) = graph.object_id(&p.src) {
        let laws = crate::laws::derive(&graph, &[]);
        let reached = laws.arrivals.contains_key(&src);
        let taken = laws.used.contains(&new);
        let needs = &graph.object(graph.arrow(new).dst).entry.needs;
        if reached && !taken && !needs.is_empty() {
            checks.push(Check {
                check: "contract".into(),
                subject: format!("entry of {}", p.dst),
                outcome: Outcome::Pass,
                reason: format!(
                    "no walk could take it: {} needs {} on entry, which no path to {} provides",
                    p.dst,
                    needs.join(", "),
                    p.src
                ),
                p: None,
                witness: None,
            });
        }
    }
    let new_object = cat.object_id(&p.dst).is_err();
    if extended.is_some() {
        checks.push(Check {
            check: "well_formed".into(),
            subject: label,
            outcome: Outcome::Pass,
            reason: if new_object {
                format!("valid names and types; introduces the new object {}", p.dst)
            } else {
                "valid names and types".into()
            },
            p: None,
            witness: None,
        });
    }

    for inv in cat.invariants() {
        let id = |n: &str| graph.object_id(n).ok();
        let (witness, holds_because) = match inv {
            Invariant::Via { from, to, through } => {
                let avoid: Vec<_> = through.iter().filter_map(|n| id(n)).collect();
                let w = id(from)
                    .zip(id(to))
                    .and_then(|(f, t)| graph.reachable(f, t, &avoid));
                (
                    w,
                    format!(
                        "every path from {from} to {to} still passes through {}",
                        through.join(" or ")
                    ),
                )
            }
            Invariant::Never { from, to } => {
                let w = id(from)
                    .zip(id(to))
                    .and_then(|(f, t)| graph.reachable(f, t, &[]));
                (w, format!("there is still no path from {from} to {to}"))
            }
            Invariant::Rule(_) => continue,
        };
        checks.push(match witness {
            Some(path) => Check {
                check: "invariant".into(),
                subject: inv.to_string(),
                outcome: Outcome::Fail,
                reason: format!(
                    "adding it creates `{}`, which breaks the invariant",
                    path.display_typed(&graph).replace("__proposed", "")
                ),
                p: None,
                witness: Some(path.display(&graph).replace("__proposed", "")),
            },
            None => Check {
                check: "invariant".into(),
                subject: inv.to_string(),
                outcome: Outcome::Pass,
                reason: holds_because,
                p: None,
                witness: None,
            },
        });
    }
    (checks, extended)
}
