//! Frame claims: which concurrent decisions may proceed together.
//!
//! Every step claims a *footprint*: the object it decides at plus the
//! targets of that object's arrows (every node the decision could touch).
//! A claim is `Read` (System 1 choosing over an unchanged frame) or `Write`
//! (System 2 proposing arrows that would extend the frame).
//!
//! Two claims *intersect* when their footprints share a node. Whether an
//! intersection makes the later claim wait depends on the [`Policy`]. Every
//! intersection is recorded as a [`Potentiality`], waited on or not, so the
//! log shows where concurrent walks could have converged.

use std::collections::{BTreeSet, HashMap};
use std::pin::pin;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use onto_core::{Category, ObjId};
use serde::Serialize;
use tokio::sync::Notify;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Read,
    Write,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Policy {
    /// Any footprint intersection waits: decision frames that could
    /// converge on a node are decided one after another.
    #[default]
    Exclusive,
    /// Only a write at the same frame waits; other intersections proceed in
    /// parallel and are logged.
    Shared,
}

impl Policy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Exclusive => "exclusive",
            Self::Shared => "shared",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Claim {
    pub walk: u64,
    pub frame: ObjId,
    pub footprint: BTreeSet<ObjId>,
    pub mode: Mode,
}

impl Claim {
    pub fn new(cat: &Category, walk: u64, frame: ObjId, mode: Mode) -> Self {
        let mut footprint: BTreeSet<ObjId> =
            cat.out(frame).iter().map(|a| cat.arrow(*a).dst).collect();
        footprint.insert(frame);
        Self {
            walk,
            frame,
            footprint,
            mode,
        }
    }

    fn intersection(&self, other: &Claim) -> BTreeSet<ObjId> {
        self.footprint
            .intersection(&other.footprint)
            .copied()
            .collect()
    }

    fn blocks(&self, other: &Claim, policy: Policy) -> bool {
        match policy {
            Policy::Exclusive => !self.footprint.is_disjoint(&other.footprint),
            Policy::Shared => {
                self.frame == other.frame && (self.mode == Mode::Write || other.mode == Mode::Write)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PotentialityKind {
    /// Two footprints share nodes.
    Node,
    /// Two walks proposed the same new concept.
    Conceptual,
    /// An arrow that held (noul) but was not followed.
    Alternative,
}

impl PotentialityKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Node => "node",
            Self::Conceptual => "conceptual",
            Self::Alternative => "alternative",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Resolution {
    /// The later claim waited for the earlier one to finish.
    Waited,
    /// Both proceeded; the intersection is only recorded.
    Coexisted,
    /// A possible step was seen and left untaken.
    NotFollowed,
}

impl Resolution {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Waited => "waited",
            Self::Coexisted => "coexisted",
            Self::NotFollowed => "not_followed",
        }
    }
}

/// A place where two concurrent walks could meet.
#[derive(Clone, Debug, Serialize)]
pub struct Potentiality {
    pub kind: PotentialityKind,
    pub resolution: Resolution,
    /// The claim that met the intersection (`None` for conceptual ones).
    pub mode: Option<Mode>,
    pub walk: u64,
    /// The other walk, for intersections between two walks.
    pub with: Option<u64>,
    pub at: String,
    pub nodes: Vec<String>,
    pub wait_ms: f64,
}

pub struct FrameLocks {
    policy: Policy,
    held: Mutex<HashMap<u64, Claim>>,
    released: Notify,
    log: Mutex<Vec<Potentiality>>,
}

/// Releases its claim on drop and wakes waiting walks.
pub struct Guard<'a> {
    locks: &'a FrameLocks,
    walk: u64,
    pub waited: Duration,
}

impl Drop for Guard<'_> {
    fn drop(&mut self) {
        self.locks.held.lock().unwrap().remove(&self.walk);
        self.locks.released.notify_waiters();
    }
}

impl FrameLocks {
    pub fn new(policy: Policy) -> Self {
        Self {
            policy,
            held: Mutex::new(HashMap::new()),
            released: Notify::new(),
            log: Mutex::new(Vec::new()),
        }
    }

    pub fn policy(&self) -> Policy {
        self.policy
    }

    /// Waits until no held claim blocks `claim`, then holds it. A walk holds
    /// at most one claim at a time and claims are all-or-nothing over the
    /// footprint, so claims cannot deadlock.
    pub async fn acquire<'a>(&'a self, cat: &Category, claim: Claim) -> Guard<'a> {
        let start = Instant::now();
        // Blocking walk -> nodes shared with it, for the potentiality log.
        let mut blocked_by: HashMap<u64, BTreeSet<ObjId>> = HashMap::new();
        let mut released = pin!(self.released.notified());
        loop {
            // Register for wakeups before checking, so a release between the
            // check and the await is not lost.
            released.as_mut().enable();
            {
                let mut held = self.held.lock().unwrap();
                let blockers: Vec<&Claim> = held
                    .values()
                    .filter(|h| h.walk != claim.walk && claim.blocks(h, self.policy))
                    .collect();
                if blockers.is_empty() {
                    let waited = start.elapsed();
                    let coexisting: Vec<(u64, BTreeSet<ObjId>)> = held
                        .values()
                        .filter(|h| h.walk != claim.walk)
                        .map(|h| (h.walk, claim.intersection(h)))
                        .filter(|(_, nodes)| !nodes.is_empty())
                        .collect();
                    let walk = claim.walk;
                    let frame = claim.frame;
                    let mode = claim.mode;
                    held.insert(walk, claim);
                    drop(held);
                    for (with, nodes) in blocked_by {
                        self.record(
                            cat,
                            Resolution::Waited,
                            mode,
                            walk,
                            with,
                            frame,
                            &nodes,
                            waited,
                        );
                    }
                    for (with, nodes) in coexisting {
                        self.record(
                            cat,
                            Resolution::Coexisted,
                            mode,
                            walk,
                            with,
                            frame,
                            &nodes,
                            Duration::ZERO,
                        );
                    }
                    return Guard {
                        locks: self,
                        walk,
                        waited,
                    };
                }
                for b in blockers {
                    blocked_by
                        .entry(b.walk)
                        .or_default()
                        .extend(claim.intersection(b));
                }
            }
            released.as_mut().await;
            released.set(self.released.notified());
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn record(
        &self,
        cat: &Category,
        resolution: Resolution,
        mode: Mode,
        walk: u64,
        with: u64,
        at: ObjId,
        nodes: &BTreeSet<ObjId>,
        wait: Duration,
    ) {
        let names = nodes.iter().map(|n| cat.object(*n).name.clone()).collect();
        self.push(Potentiality {
            kind: PotentialityKind::Node,
            resolution,
            mode: Some(mode),
            walk,
            with: Some(with),
            at: cat.object(at).name.clone(),
            nodes: names,
            wait_ms: (wait.as_secs_f64() * 1e6).round() / 1e3,
        });
    }

    pub(crate) fn push(&self, p: Potentiality) {
        tracing::info!(
            target: "onto",
            event = "potentiality",
            kind = p.kind.as_str(),
            resolution = p.resolution.as_str(),
            mode = p.mode.map(Mode::as_str),
            walk = p.walk,
            with = p.with,
            at = %p.at,
            nodes = %p.nodes.join(","),
            wait_ms = p.wait_ms,
        );
        self.log.lock().unwrap().push(p);
    }

    pub fn potentialities(&self) -> Vec<Potentiality> {
        self.log.lock().unwrap().clone()
    }
}
