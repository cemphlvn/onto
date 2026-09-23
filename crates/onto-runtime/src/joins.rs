//! Joins: how sibling branches of one fork recombine (`docs/03-joins.md`).
//!
//! A walk arriving at a join object waits (holding no frame claim) until
//! the join's policy resolves for its innermost fork:
//!
//! - **all**: every sibling has arrived; the lowest-id walk continues with
//!   the intersection of the siblings' tokens; the others end, joined in.
//!   If a sibling ends elsewhere (or waits at another join), the join is
//!   incomplete and the arrived walks escalate (`incomplete_join`).
//! - **race**: the first arrival continues with its own tokens; later
//!   arrivals end as having lost the race. Losers are not interrupted.
//! - **gate**: the branch spawned along the authority arrow delivers its
//!   tokens and ends; a content walk continues with
//!   (content ∩ authority) ∪ (authority ∩ export). If the authority branch
//!   ends elsewhere, content walks escalate (`blocked_by_gate`).
//!
//! Deadlock freedom: a walk only ever waits on siblings that are still
//! running. A sibling that is waiting at another join, or has ended,
//! resolves the join as incomplete/blocked instead, so waits cannot form
//! a cycle, and every running walk ends within its step limit.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::pin::pin;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use onto_core::{Join, ObjId};
use tokio::sync::Notify;

pub type Tokens = BTreeSet<String>;

/// One fork's sibling set: the walk that forked and the branches it
/// spawned, with the fork arrow each one took.
#[derive(Clone, Debug)]
pub struct ForkInfo {
    /// The fork's frame record id.
    pub record: String,
    pub members: Vec<u64>,
    pub arrow_of: BTreeMap<u64, String>,
}

#[derive(Clone, Debug)]
pub struct Arrival {
    pub tokens: Tokens,
    /// The walk's last frame record before the join (for causal links).
    pub last_record: Option<String>,
    /// Attestations that opened the arrow this walk arrived by (empty: the
    /// arrival rests on judgment alone).
    pub attested: Vec<String>,
}

/// What a walk does after its join resolves.
#[derive(Clone, Debug, PartialEq)]
pub enum JoinOutcome {
    /// Continue past the join with these tokens; `merged` are the other
    /// walks folded in (with their last records).
    Continue {
        tokens: Tokens,
        merged: Vec<(u64, Option<String>)>,
        /// Every arrival at the join with its attestations (this walk's too).
        arrivals: Vec<(u64, Vec<String>)>,
    },
    /// This walk ends here: `into` continues for it.
    End { into: Option<u64>, reason: String },
    /// The join cannot complete for this walk.
    Escalate { incomplete: bool, reason: String },
}

#[derive(Clone, Debug, PartialEq)]
enum Status {
    Running,
    AtJoin(String, ObjId),
    Ended,
}

#[derive(Default)]
struct Table {
    arrivals: BTreeMap<u64, Arrival>,
    resolution: Option<Resolution>,
}

#[derive(Clone)]
enum Resolution {
    All { continuing: u64, tokens: Tokens },
    Incomplete { missing: Vec<u64> },
    Race { winner: u64 },
}

#[derive(Default)]
pub struct Joins {
    inner: Mutex<Inner>,
    changed: Notify,
}

#[derive(Default)]
struct Inner {
    status: HashMap<u64, Status>,
    tables: HashMap<(String, ObjId), Table>,
}

impl Joins {
    pub fn started(&self, walk: u64) {
        self.inner
            .lock()
            .unwrap()
            .status
            .insert(walk, Status::Running);
        self.changed.notify_waiters();
    }

    pub fn ended(&self, walk: u64) {
        self.inner
            .lock()
            .unwrap()
            .status
            .insert(walk, Status::Ended);
        self.changed.notify_waiters();
    }

    /// Arrives at join object `at` for `fork` and waits until the policy
    /// resolves for this walk. Returns the outcome and the time waited.
    pub async fn arrive(
        &self,
        walk: u64,
        fork: &ForkInfo,
        at: ObjId,
        join: &Join,
        arrival: Arrival,
    ) -> (JoinOutcome, Duration) {
        let start = Instant::now();
        let key = (fork.record.clone(), at);
        let mut changed = pin!(self.changed.notified());
        let mut first = true;
        loop {
            changed.as_mut().enable();
            {
                let mut inner = self.inner.lock().unwrap();
                if first {
                    inner
                        .status
                        .insert(walk, Status::AtJoin(fork.record.clone(), at));
                    inner
                        .tables
                        .entry(key.clone())
                        .or_default()
                        .arrivals
                        .insert(walk, arrival.clone());
                    first = false;
                    self.changed.notify_waiters();
                }
                if let Some(outcome) = resolve(&mut inner, walk, fork, &key, join) {
                    // A walk that continues past the join is running again
                    // (otherwise a later join would see it as waiting
                    // elsewhere). Walks that end are marked by `ended`.
                    if matches!(outcome, JoinOutcome::Continue { .. }) {
                        inner.status.insert(walk, Status::Running);
                    }
                    drop(inner);
                    self.changed.notify_waiters();
                    return (outcome, start.elapsed());
                }
            }
            changed.as_mut().await;
            changed.set(self.changed.notified());
        }
    }
}

/// A sibling that can no longer arrive at this join.
fn gone(inner: &Inner, key: &(String, ObjId), m: u64) -> bool {
    match inner.status.get(&m) {
        Some(Status::Ended) => true,
        Some(Status::AtJoin(f, o)) => (f.clone(), *o) != *key,
        _ => false, // running, or spawned and not yet started
    }
}

fn resolve(
    inner: &mut Inner,
    walk: u64,
    fork: &ForkInfo,
    key: &(String, ObjId),
    join: &Join,
) -> Option<JoinOutcome> {
    let merged_except = |table: &Table, w: u64| -> Vec<(u64, Option<String>)> {
        table
            .arrivals
            .iter()
            .filter(|(id, _)| **id != w)
            .map(|(id, a)| (*id, a.last_record.clone()))
            .collect()
    };
    match join {
        Join::All => {
            if inner.tables[key].resolution.is_none() {
                let arrived: BTreeSet<u64> = inner.tables[key].arrivals.keys().copied().collect();
                let missing: Vec<u64> = fork
                    .members
                    .iter()
                    .copied()
                    .filter(|m| !arrived.contains(m) && gone(inner, key, *m))
                    .collect();
                let resolution = if !missing.is_empty() {
                    Resolution::Incomplete { missing }
                } else if fork.members.iter().all(|m| arrived.contains(m)) {
                    let table = &inner.tables[key];
                    let mut sets = table.arrivals.values().map(|a| &a.tokens);
                    let first = sets.next().cloned().unwrap_or_default();
                    let tokens = sets.fold(first, |acc, t| acc.intersection(t).cloned().collect());
                    Resolution::All {
                        continuing: *arrived.iter().next().expect("arrived"),
                        tokens,
                    }
                } else {
                    return None; // wait for running siblings
                };
                inner.tables.get_mut(key).unwrap().resolution = Some(resolution);
            }
            let table = &inner.tables[key];
            Some(match table.resolution.clone().unwrap() {
                Resolution::All { continuing, tokens } if continuing == walk => {
                    JoinOutcome::Continue {
                        tokens,
                        merged: merged_except(table, walk),
                        arrivals: attestations(table),
                    }
                }
                Resolution::All { continuing, .. } => JoinOutcome::End {
                    into: Some(continuing),
                    reason: format!("joined into walk {continuing}"),
                },
                Resolution::Incomplete { missing } => JoinOutcome::Escalate {
                    incomplete: true,
                    reason: format!(
                        "sibling walk(s) {} ended without arriving",
                        missing
                            .iter()
                            .map(u64::to_string)
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                },
                Resolution::Race { .. } => unreachable!("all-join resolved as race"),
            })
        }
        Join::Race => {
            let table = inner.tables.get_mut(key).unwrap();
            let winner = match &table.resolution {
                Some(Resolution::Race { winner }) => *winner,
                _ => {
                    table.resolution = Some(Resolution::Race { winner: walk });
                    walk
                }
            };
            Some(if winner == walk {
                JoinOutcome::Continue {
                    tokens: table.arrivals[&walk].tokens.clone(),
                    merged: Vec::new(),
                    arrivals: vec![(walk, table.arrivals[&walk].attested.clone())],
                }
            } else {
                JoinOutcome::End {
                    into: Some(winner),
                    reason: format!("lost the race to walk {winner}"),
                }
            })
        }
        Join::Gate { authority, export } => {
            let authority_walk = fork
                .arrow_of
                .iter()
                .find(|(_, a)| *a == authority)
                .map(|(w, _)| *w);
            if Some(walk) == authority_walk {
                return Some(JoinOutcome::End {
                    into: None,
                    reason: format!("delivered authority for {}", export_list(export)),
                });
            }
            let Some(auth) = authority_walk else {
                return Some(JoinOutcome::Escalate {
                    incomplete: false,
                    reason: format!(
                        "no sibling was spawned along the authority arrow `{authority}`"
                    ),
                });
            };
            let table = &inner.tables[key];
            if let Some(a) = table.arrivals.get(&auth) {
                let content = &table.arrivals[&walk].tokens;
                let mut tokens: Tokens = content.intersection(&a.tokens).cloned().collect();
                tokens.extend(a.tokens.iter().filter(|t| export.contains(t)).cloned());
                return Some(JoinOutcome::Continue {
                    tokens,
                    merged: vec![(auth, a.last_record.clone())],
                    // For a gate, the evidence that matters is the authority's.
                    arrivals: vec![(auth, a.attested.clone())],
                });
            }
            if gone(inner, key, auth) {
                return Some(JoinOutcome::Escalate {
                    incomplete: false,
                    reason: format!(
                        "the authority branch (walk {auth}, along `{authority}`) ended without arriving"
                    ),
                });
            }
            None
        }
    }
}

fn attestations(table: &Table) -> Vec<(u64, Vec<String>)> {
    table
        .arrivals
        .iter()
        .map(|(w, a)| (*w, a.attested.clone()))
        .collect()
}

fn export_list(export: &[String]) -> String {
    if export.is_empty() {
        "nothing".into()
    } else {
        export.join(", ")
    }
}
