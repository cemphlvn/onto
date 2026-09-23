//! The core loop: many walks at once, each step claiming its frame,
//! System-1 judgments first, System-2 proposals on escalation. Noul frames
//! may fork a walk into parallel branches.

use std::collections::{BTreeSet, HashMap};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering::Relaxed};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use onto_core::category::Closure;
use onto_core::walk::{
    Answer, Decision, Disposition, Escalation, Proposal, candidates, decide, dispose,
};
use onto_core::{ArrowId, Category, ObjId, Path, Primitive};
use serde::Serialize;
use serde_json::Value;
use tokio::sync::{Semaphore, mpsc};
use tokio::task::JoinSet;

use crate::frames::{Claim, FrameLocks, Mode, Policy, Potentiality, PotentialityKind, Resolution};
use crate::joins::{Arrival, ForkInfo, JoinOutcome, Joins};
use crate::mem::{self, MemSample};
use crate::model::{
    Candidate, Critic, Focus, FrameRequest, Hop, Judge, ModelError, Pending, ProposalRequest,
    Proposer, Usage,
};
use crate::record::{self, ClaimRecord, FrameRecord, JudgeRecord, Outcome};

#[derive(Clone, Debug)]
pub struct Job {
    pub goal: String,
    pub from: String,
    /// Structured facts about the case; `require` clauses read it.
    pub case: Value,
}

#[derive(Clone, Debug)]
pub struct Config {
    /// Minimum System-1 confidence to act without escalating.
    pub threshold: f32,
    pub policy: Policy,
    /// Start the System-2 call alongside System 1 at every closed frame and
    /// cancel it if System 1 is confident. Lower latency, more tokens.
    pub speculate: bool,
    pub max_steps: usize,
    /// Extra branches one job may fork into, across all its forks.
    pub max_branches: usize,
    /// Forks along one lineage (a branch of a branch of …).
    pub max_fork_depth: usize,
    /// In-flight call limits per provider.
    pub judge_concurrency: usize,
    pub proposer_concurrency: usize,
    pub mem_sample_every: Duration,
    /// Open world (the default): when a frame's enumeration is missing,
    /// System 2's proposals that pass the supervisor (every structural
    /// proof, and no failed semantic check) join the live graph as
    /// *learned* structure and the walk keeps going. Closed world: the
    /// declared graph only; proposals stay provisional for a person.
    pub open_world: bool,
    /// Frames one walk may extend (open world).
    pub max_expansions: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            threshold: 0.6,
            policy: Policy::default(),
            speculate: false,
            max_steps: 16,
            max_branches: 4,
            max_fork_depth: 2,
            judge_concurrency: 16,
            proposer_concurrency: 4,
            mem_sample_every: Duration::from_millis(250),
            open_world: true,
            max_expansions: 3,
        }
    }
}

/// An arrow with its probability, for reports.
#[derive(Clone, Debug, Serialize)]
pub struct Alt {
    pub arrow: String,
    pub to: String,
    pub p: f32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "step", rename_all = "snake_case")]
pub enum StepRecord {
    Followed {
        from: String,
        arrow: String,
        to: String,
        decided_by: Primitive,
        p: f32,
        confidence: Option<f32>,
        /// Arrows that also held (noul) but were not pursued.
        alternatives: Vec<Alt>,
        /// Attestations that opened this arrow (empty: judgment alone).
        attested: Vec<String>,
        /// The arrow is learned structure (open world), not declared.
        learned: bool,
        wait_ms: f64,
    },
    Forked {
        at: String,
        /// The first branch continues this walk; the others are `spawned`.
        branches: Vec<Alt>,
        fork_p: f32,
        spawned: Vec<u64>,
        wait_ms: f64,
    },
    Escalated {
        at: String,
        reason: Escalation,
        proposals: Vec<Proposal>,
        wait_ms: f64,
    },
    Failed {
        at: String,
        error: String,
    },
    /// Open world: the frame's enumeration was missing, System 2 proposed,
    /// and the supervisor admitted `learned` into the live graph. The walk
    /// re-judges the frame with them.
    Expanded {
        at: String,
        reason: Escalation,
        learned: Vec<Proposal>,
        /// Proposals the supervisor refused, with the reason.
        refused: Vec<(String, String)>,
        wait_ms: f64,
    },
    /// Arrived at a join object and the join resolved for this walk.
    Joined {
        at: String,
        policy: String,
        /// `continued`, `ended` (folded into `into`) or `escalated`.
        role: String,
        /// For the continuing walk: every branch arrived by an attested
        /// arrow, so the join is evidence about the world, not only the walk.
        completion_attested: bool,
        into: Option<u64>,
        with: Vec<u64>,
        tokens: Vec<String>,
        detail: String,
        wait_ms: f64,
    },
}

#[derive(Clone, Debug, Serialize)]
pub struct WalkReport {
    pub walk: u64,
    /// The walk this one branched from, if it is a branch.
    pub parent: Option<u64>,
    pub goal: String,
    pub from: String,
    pub path: String,
    pub steps: Vec<StepRecord>,
    /// One disposition record per frame visit (the provenance artifact).
    pub frames: Vec<FrameRecord>,
    pub elapsed_ms: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct RunReport {
    /// SHA-256 of the category source every walk in this run used.
    pub snapshot: Option<String>,
    pub judge: String,
    pub proposer: String,
    pub policy: Policy,
    pub speculate: bool,
    /// Roots first, then branches, each in id order.
    pub walks: Vec<WalkReport>,
    /// Structure the open world learned in this run, in admission order.
    pub learned: Vec<Learned>,
    /// Decisions this run made at frames that declare `memory`: the
    /// precedents later runs may be shown.
    pub precedents: Vec<crate::memory::Precedent>,
    pub potentialities: Vec<Potentiality>,
    pub wall_ms: f64,
    /// Sum of every model call's latency; `model_ms_sum / wall_ms` is the
    /// effective parallelism.
    pub model_ms_sum: f64,
    pub judge_calls: u64,
    pub judge_questions: u64,
    pub proposer_calls: u64,
    pub speculative_discarded: u64,
    pub forks: u64,
    pub branches: u64,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub mem_start: MemSample,
    pub mem_end: MemSample,
    pub peak_rss_bytes: Option<usize>,
}

/// One arrow the open world admitted: the learned layer's unit. Replayed
/// through the same structural proofs whenever the layer is loaded.
#[derive(Clone, Debug, Serialize, serde::Deserialize)]
pub struct Learned {
    pub proposal: Proposal,
    /// The frame record whose escalation it answered.
    pub record: String,
    pub reason: String,
    /// Snapshot of the graph the run started from.
    pub snapshot: Option<String>,
    /// The supervisor's checks, `check: outcome` (unknowns included).
    pub checks: Vec<String>,
}

#[derive(Default)]
struct Counters {
    model_us: AtomicU64,
    judge_calls: AtomicU64,
    judge_questions: AtomicU64,
    proposer_calls: AtomicU64,
    speculative_discarded: AtomicU64,
    forks: AtomicU64,
    branches: AtomicU64,
    joins: AtomicU64,
    tokens_in: AtomicU64,
    tokens_out: AtomicU64,
}

/// Where a walk starts: a job's root, or a branch from a fork.
struct Seed {
    id: u64,
    parent: Option<u64>,
    job: Job,
    path: Path,
    hops: Vec<Hop>,
    depth: usize,
    /// Extra branches this job may still spawn, shared by its lineage.
    budget: Arc<AtomicUsize>,
    /// The frame record this walk's first visit follows (a branch's fork).
    after: Option<String>,
    /// The aspect a branch handles.
    focus: Option<Focus>,
    /// Capability tokens held when the walk starts (a branch inherits its
    /// parent's, including the effects of the arrow it forked along).
    tokens: BTreeSet<String>,
    /// Enclosing forks, innermost last: whom this walk may join with.
    forks: Vec<Arc<ForkInfo>>,
}

pub struct Engine<J, P> {
    /// The live graph: the declared category plus whatever the open world
    /// has learned in this engine's lifetime. Extension only appends
    /// objects and arrows, so ids stay stable across versions.
    graph: std::sync::RwLock<Arc<Category>>,
    /// The loaded snapshot (declared + learned layer at start): the identity
    /// every record refers to.
    snapshot: Option<String>,
    /// Arrows learned in this engine's lifetime, for the learned layer file.
    learned: Mutex<Vec<Learned>>,
    /// Precedents frames with `memory: similar N` may be shown: loaded
    /// ones first, then this engine's own decisions.
    memory: Mutex<Vec<crate::memory::Precedent>>,
    /// How many of `memory` were loaded (the rest are new).
    memory_loaded: AtomicUsize,
    judge: J,
    proposer: P,
    cfg: Config,
    locks: FrameLocks,
    judge_slots: Semaphore,
    proposer_slots: Semaphore,
    /// Proposed concept key -> walks that proposed it.
    concepts: Mutex<HashMap<String, Vec<u64>>>,
    /// Provisional proposals per frame, shown to later proposers there.
    pending: Mutex<HashMap<ObjId, Vec<(u64, Proposal)>>>,
    joins: Joins,
    counters: Counters,
    /// Walk ids are unique across `run` calls on one engine, so conceptual
    /// intersections are detected between runs too.
    next_walk: AtomicU64,
}

impl<J: Judge + Critic, P: Proposer> Engine<J, P> {
    /// The current graph (declared + learned so far).
    pub fn cat(&self) -> Arc<Category> {
        self.graph.read().unwrap().clone()
    }

    /// Loads precedents (from an earlier run's memory file).
    pub fn remember(&self, precedents: Vec<crate::memory::Precedent>) {
        let mut m = self.memory.lock().unwrap();
        m.extend(precedents);
        self.memory_loaded.store(m.len(), Relaxed);
    }

    /// Arrows the open world has learned so far.
    pub fn learned(&self) -> Vec<Learned> {
        self.learned.lock().unwrap().clone()
    }

    pub fn new(cat: Arc<Category>, judge: J, proposer: P, cfg: Config) -> Arc<Self> {
        Arc::new(Self {
            locks: FrameLocks::new(cfg.policy),
            judge_slots: Semaphore::new(cfg.judge_concurrency),
            proposer_slots: Semaphore::new(cfg.proposer_concurrency),
            concepts: Mutex::new(HashMap::new()),
            pending: Mutex::new(HashMap::new()),
            joins: Joins::default(),
            counters: Counters::default(),
            next_walk: AtomicU64::new(1),
            snapshot: cat.snapshot().map(str::to_owned),
            graph: std::sync::RwLock::new(cat),
            learned: Mutex::new(Vec::new()),
            memory: Mutex::new(Vec::new()),
            memory_loaded: AtomicUsize::new(0),
            judge,
            proposer,
            cfg,
        })
    }

    /// Runs every job, and every branch they fork, concurrently; reports
    /// once all have finished.
    pub async fn run(self: &Arc<Self>, jobs: Vec<Job>) -> Result<RunReport, onto_core::Error> {
        let starts = jobs
            .iter()
            .map(|j| self.cat().object_id(&j.from))
            .collect::<Result<Vec<_>, _>>()?;
        let learned_before = self.learned.lock().unwrap().len();
        let memory_before = self.memory.lock().unwrap().len();
        let mem_start = mem::sample();
        let sampler = mem::spawn_sampler(self.cfg.mem_sample_every);
        tracing::info!(
            target: "onto",
            event = "run.start",
            category = self.cat().name(),
            snapshot = self.snapshot.as_deref(),
            walks = jobs.len(),
            judge = %Judge::name(&self.judge),
            proposer = %self.proposer.name(),
            policy = self.cfg.policy.as_str(),
            speculate = self.cfg.speculate,
            open_world = self.cfg.open_world,
            threshold = r4(self.cfg.threshold),
            max_branches = self.cfg.max_branches,
            max_fork_depth = self.cfg.max_fork_depth,
        );
        let t0 = Instant::now();

        // Walks send branch seeds here when they fork; this loop spawns
        // them, so walks never spawn themselves.
        let (branch_tx, mut branch_rx) = mpsc::unbounded_channel::<Seed>();
        let mut running = JoinSet::new();
        for (job, from) in jobs.into_iter().zip(starts) {
            let seed = Seed {
                id: self.next_walk.fetch_add(1, Relaxed),
                parent: None,
                job,
                path: Path::id(from),
                hops: Vec::new(),
                depth: 0,
                budget: Arc::new(AtomicUsize::new(self.cfg.max_branches)),
                after: None,
                focus: None,
                tokens: BTreeSet::new(),
                forks: Vec::new(),
            };
            running.spawn(self.clone().walk(seed, branch_tx.clone()));
        }
        let mut walks = Vec::new();
        loop {
            tokio::select! {
                Some(seed) = branch_rx.recv() => {
                    running.spawn(self.clone().walk(seed, branch_tx.clone()));
                }
                done = running.join_next() => match done {
                    Some(report) => walks.push(report.expect("walk task panicked")),
                    // Every seed is sent before its parent finishes, so an
                    // empty set plus an empty queue means all walks are done.
                    None => match branch_rx.try_recv() {
                        Ok(seed) => {
                            running.spawn(self.clone().walk(seed, branch_tx.clone()));
                        }
                        Err(_) => break,
                    },
                },
            }
        }
        walks.sort_by_key(|w| (w.parent.is_some(), w.walk));

        let wall = t0.elapsed();
        let mem_end = mem::sample();
        let peak_rss_bytes = sampler.stop().max(mem_end.rss_bytes);
        let c = &self.counters;
        let report = RunReport {
            snapshot: self.snapshot.clone(),
            judge: Judge::name(&self.judge),
            proposer: self.proposer.name(),
            policy: self.cfg.policy,
            speculate: self.cfg.speculate,
            walks,
            learned: self.learned()[learned_before..].to_vec(),
            precedents: self.memory.lock().unwrap()[memory_before..].to_vec(),
            potentialities: self.locks.potentialities(),
            wall_ms: ms(wall),
            model_ms_sum: c.model_us.load(Relaxed) as f64 / 1e3,
            judge_calls: c.judge_calls.load(Relaxed),
            judge_questions: c.judge_questions.load(Relaxed),
            proposer_calls: c.proposer_calls.load(Relaxed),
            speculative_discarded: c.speculative_discarded.load(Relaxed),
            forks: c.forks.load(Relaxed),
            branches: c.branches.load(Relaxed),
            tokens_in: c.tokens_in.load(Relaxed),
            tokens_out: c.tokens_out.load(Relaxed),
            mem_start,
            mem_end,
            peak_rss_bytes,
        };
        tracing::info!(
            target: "onto",
            event = "run.end",
            wall_ms = report.wall_ms,
            model_ms_sum = report.model_ms_sum,
            judge_calls = report.judge_calls,
            judge_questions = report.judge_questions,
            proposer_calls = report.proposer_calls,
            speculative_discarded = report.speculative_discarded,
            forks = report.forks,
            branches = report.branches,
            potentialities = report.potentialities.len(),
            tokens_in = report.tokens_in,
            tokens_out = report.tokens_out,
            peak_rss_bytes = report.peak_rss_bytes,
            heap_peak_bytes = report.mem_end.heap_peak_bytes,
        );
        Ok(report)
    }

    async fn walk(
        self: Arc<Self>,
        seed: Seed,
        branch_tx: mpsc::UnboundedSender<Seed>,
    ) -> WalkReport {
        let cat_arc = self.cat();
        let cat = &*cat_arc;
        let t0 = Instant::now();
        let Seed {
            id,
            parent,
            job,
            mut path,
            mut hops,
            depth,
            budget,
            mut after,
            mut focus,
            mut tokens,
            mut forks,
        } = seed;
        self.joins.started(id);
        let mut seq = 0usize;
        // Attestations behind the arrow this walk last followed.
        let mut last_attested: Vec<String> = Vec::new();
        tracing::info!(
            target: "onto",
            event = "walk.start",
            walk = id,
            parent,
            from = %cat.object(path.dst).name,
            goal = %job.goal,
        );
        let mut steps = Vec::new();
        let mut frames: Vec<FrameRecord> = Vec::new();

        // Starting at an object is entering it: a new walk may not start
        // where the entry contract fails (it would skip the contract).
        if parent.is_none() {
            let start = cat.object(path.dst);
            let missing: Vec<&str> = start
                .entry
                .needs
                .iter()
                .filter(|t| !tokens.contains(*t))
                .map(String::as_str)
                .collect();
            let require_failed = start
                .entry
                .require
                .as_ref()
                .is_some_and(|r| !r.eval(&job.case));
            if !missing.is_empty() || require_failed {
                let mut why = Vec::new();
                if !missing.is_empty() {
                    why.push(format!("it needs {} on entry", missing.join(", ")));
                }
                if require_failed {
                    let r = start
                        .entry
                        .require
                        .as_ref()
                        .map_or(String::new(), ToString::to_string);
                    why.push(format!("it requires `{r}` on entry"));
                }
                let error = format!("cannot start at {}: {}", start.name, why.join(" and "));
                tracing::warn!(target: "onto", event = "walk.refused", walk = id, at = %start.name, error = %error);
                steps.push(StepRecord::Failed {
                    at: start.name.clone(),
                    error,
                });
                self.joins.ended(id);
                return WalkReport {
                    walk: id,
                    parent,
                    goal: job.goal,
                    from: job.from,
                    path: path.display_typed(cat),
                    steps,
                    frames,
                    elapsed_ms: ms(t0.elapsed()),
                };
            }
        }

        let mut expansions = 0usize;
        'steps: for _ in 0..self.cfg.max_steps {
            // The graph may have grown since the last step (open world).
            let cat_arc = self.cat();
            let cat = &*cat_arc;
            let at = path.dst;
            let object = cat.object(at);

            // A join object: recombine with this walk's innermost fork.
            if let (Some(join), Some(fork)) = (&object.join, forks.last().cloned()) {
                let arrival = Arrival {
                    tokens: tokens.clone(),
                    last_record: frames.last().map(|f: &FrameRecord| f.id.clone()),
                    attested: last_attested.clone(),
                };
                let (outcome, waited) = self.joins.arrive(id, &fork, at, join, arrival).await;
                self.counters.joins.fetch_add(1, Relaxed);
                seq += 1;
                let join_id = format!("w{id}.{seq}");
                let mut completion_attested = false;
                let (role, into, with, detail, merged_from, escalate) = match &outcome {
                    JoinOutcome::Continue {
                        tokens: t,
                        merged,
                        arrivals,
                    } => {
                        tokens = t.clone();
                        forks.pop();
                        let with: Vec<u64> = merged.iter().map(|m| m.0).collect();
                        let from: Vec<String> = merged.iter().filter_map(|m| m.1.clone()).collect();
                        // A join is evidence about the world only if every
                        // branch arrived by an attested arrow.
                        let unattested: Vec<String> = arrivals
                            .iter()
                            .filter(|(_, a)| a.is_empty())
                            .map(|(w, _)| w.to_string())
                            .collect();
                        completion_attested = unattested.is_empty();
                        let by: Vec<String> = arrivals
                            .iter()
                            .map(|(w, a)| format!("walk {w}: {}", a.join(", ")))
                            .collect();
                        let evidence = match (join, completion_attested) {
                            (onto_core::Join::Gate { .. }, true) => {
                                format!("authorized by attested evidence ({})", by.join("; "))
                            }
                            (onto_core::Join::Gate { .. }, false) => {
                                "authority NOT attested (the authority branch arrived on judgment alone)".to_owned()
                            }
                            (onto_core::Join::Race, true) => format!("winner attested ({})", by.join("; ")),
                            (onto_core::Join::Race, false) => {
                                "winner NOT attested (it arrived on judgment alone)".to_owned()
                            }
                            (onto_core::Join::All, true) => format!("completion attested ({})", by.join("; ")),
                            (onto_core::Join::All, false) => format!(
                                "reached, completion NOT attested (walk {} arrived on judgment alone)",
                                unattested.join(", ")
                            ),
                        };
                        let detail = format!(
                            "{evidence}; continues holding {{{}}}",
                            tokens.iter().cloned().collect::<Vec<_>>().join(", ")
                        );
                        ("continued", None, with, detail, from, None)
                    }
                    JoinOutcome::End { into, reason } => (
                        "ended",
                        *into,
                        into.iter().copied().collect(),
                        reason.clone(),
                        Vec::new(),
                        None,
                    ),
                    JoinOutcome::Escalate { incomplete, reason } => {
                        let e = if *incomplete {
                            Escalation::IncompleteJoin
                        } else {
                            Escalation::BlockedByGate
                        };
                        (
                            "escalated",
                            None,
                            Vec::new(),
                            reason.clone(),
                            Vec::new(),
                            Some(e),
                        )
                    }
                };
                tracing::info!(
                    target: "onto",
                    event = "join",
                    walk = id,
                    at = %object.name,
                    policy = join.name(),
                    role,
                    into,
                    wait_ms = ms(waited),
                    detail = %detail,
                );
                frames.push(FrameRecord {
                    seen: None,
                    id: join_id.clone(),
                    after: after.clone(),
                    snapshot: self.snapshot.clone(),
                    walk: id,
                    focus: focus.as_ref().map(|f| f.arrow.clone()),
                    tokens: tokens.iter().cloned().collect(),
                    at: object.name.clone(),
                    primitive: object.frame.primitive,
                    closure: if object.closure == Closure::Closed {
                        "closed"
                    } else {
                        "open"
                    },
                    claim: ClaimRecord {
                        mode: Mode::Read,
                        wait_ms: ms(waited),
                    },
                    judge: None,
                    candidates: Vec::new(),
                    outcome: Outcome::Joined {
                        policy: join.name().into(),
                        role: role.into(),
                        with: with.clone(),
                        detail: detail.clone(),
                    },
                    proposals: Vec::new(),
                    merged_from,
                    attested: Vec::new(),
                });
                after = Some(join_id);
                steps.push(StepRecord::Joined {
                    at: object.name.clone(),
                    policy: join.name().into(),
                    role: role.into(),
                    completion_attested,
                    into,
                    with,
                    tokens: tokens.iter().cloned().collect(),
                    detail: detail.clone(),
                    wait_ms: ms(waited),
                });
                if let Some(reason) = escalate {
                    steps.push(StepRecord::Escalated {
                        at: object.name.clone(),
                        reason,
                        proposals: Vec::new(),
                        wait_ms: ms(waited),
                    });
                }
                if role != "continued" {
                    break 'steps;
                }
            }

            let closed = object.closure == Closure::Closed;
            if closed && cat.out(at).is_empty() {
                break; // terminal
            }
            let at_name = object.name.clone();
            let primitive = object.frame.primitive;
            let frame = candidates(cat, at, &job.case, &tokens);

            // Claim the frame. With speculation the System-2 call may start
            // right away, so the claim is a write from the start.
            let mode = if self.cfg.speculate {
                Mode::Write
            } else {
                Mode::Read
            };
            let mut guard = self.locks.acquire(cat, Claim::new(cat, id, at, mode)).await;
            let wait_ms = ms(guard.waited);
            seq += 1;
            let rec_id = format!("w{id}.{seq}");
            // The focus this visit was made under (a fork below may change it).
            let visit_focus = focus.as_ref().map(|f| f.arrow.clone());
            // Exactly what a model is shown at this visit, if one is asked.
            let visit_seen = self.state(cat, &job, &path, &hops, &focus, &tokens);
            let record = |judge: Option<JudgeRecord>, candidates, outcome| FrameRecord {
                seen: judge.is_some().then(|| visit_seen.clone()),
                id: rec_id.clone(),
                after: after.clone(),
                snapshot: self.snapshot.clone(),
                walk: id,
                focus: visit_focus.clone(),
                tokens: tokens.iter().cloned().collect(),
                at: at_name.clone(),
                primitive,
                closure: if closed { "closed" } else { "open" },
                claim: ClaimRecord { mode, wait_ms },
                judge,
                candidates,
                outcome,
                proposals: Vec::new(),
                merged_from: Vec::new(),
                attested: Vec::new(),
            };

            let mut speculative = None;
            let reason = 'judged: {
                if frame.is_empty() {
                    // No arrows, or every arrow's `require` failed.
                    let reason = if closed {
                        Escalation::NoneOfThese
                    } else {
                        Escalation::OpenFrame
                    };
                    let ds = dispose(
                        cat,
                        at,
                        &job.case,
                        &tokens,
                        &frame,
                        None,
                        &Decision::Escalate(reason),
                        self.cfg.threshold,
                        false,
                    );
                    frames.push(record(
                        None,
                        record::candidates(cat, ds),
                        Outcome::Escalated { reason },
                    ));
                    break 'judged reason;
                }
                if self.cfg.speculate {
                    let engine = self.clone();
                    let req = self.proposal_request(
                        &job,
                        &path,
                        &hops,
                        &focus,
                        &tokens,
                        "speculative: started alongside System 1",
                    );
                    speculative = Some(tokio::spawn(async move {
                        engine.call_proposer(id, req, true).await
                    }));
                }
                let split = primitive == Primitive::Split;
                let can_fork = (primitive == Primitive::Noul || split)
                    && depth < self.cfg.max_fork_depth
                    && budget.load(Relaxed) > 0
                    && frame.len() > 1;
                // A split must pursue every eligible arrow; if the branch
                // budget or depth cannot cover them all, escalate rather
                // than drop one (a partial split would let an all-join
                // complete with a check missing).
                if split
                    && frame.len() > 1
                    && (depth >= self.cfg.max_fork_depth || budget.load(Relaxed) < frame.len() - 1)
                {
                    let reason = Escalation::SplitOverBudget;
                    let ds = dispose(
                        cat,
                        at,
                        &job.case,
                        &tokens,
                        &frame,
                        None,
                        &Decision::Escalate(reason),
                        self.cfg.threshold,
                        false,
                    );
                    frames.push(record(
                        None,
                        record::candidates(cat, ds),
                        Outcome::Escalated { reason },
                    ));
                    break 'judged reason;
                }
                let (mut answer, judge_rec) = if split {
                    // No judgment: every eligible arrow holds.
                    (
                        Answer::Noul {
                            holds: vec![1.0; frame.len()],
                            fork: None,
                        },
                        None,
                    )
                } else {
                    let req =
                        self.frame_request(&job, &path, &hops, &focus, &tokens, &frame, can_fork);
                    match self.call_judge(id, req).await {
                        Ok((a, r)) => (a, Some(r)),
                        Err(e) => {
                            if let Some(h) = speculative {
                                h.abort();
                            }
                            let mut ds = dispose(
                                cat,
                                at,
                                &job.case,
                                &tokens,
                                &frame,
                                None,
                                &Decision::Escalate(Escalation::LowConfidence),
                                self.cfg.threshold,
                                false,
                            );
                            for d in ds
                                .iter_mut()
                                .filter(|d| d.disposition == Disposition::Deferred)
                            {
                                d.reason = "not judged: the judge call failed".into();
                            }
                            frames.push(record(
                                None,
                                record::candidates(cat, ds),
                                Outcome::Failed {
                                    error: e.to_string(),
                                },
                            ));
                            steps.push(self.failed(id, &at_name, e));
                            break 'steps;
                        }
                    }
                };
                // A parallel or split frame pursues every holding arrow; the
                // engine enforces this rather than trusting a judge.
                if (object.frame.parallel || split)
                    && let Answer::Noul { fork, .. } = &mut answer
                {
                    *fork = Some(1.0);
                }
                let decision = decide(object.closure, Some(&answer), self.cfg.threshold, can_fork);
                let mut ds = dispose(
                    cat,
                    at,
                    &job.case,
                    &tokens,
                    &frame,
                    Some(&answer),
                    &decision,
                    self.cfg.threshold,
                    can_fork,
                );
                let (index, p, alternatives) = match decision {
                    Decision::Escalate(reason) => {
                        frames.push(record(
                            judge_rec,
                            record::candidates(cat, ds),
                            Outcome::Escalated { reason },
                        ));
                        break 'judged reason;
                    }
                    Decision::Follow {
                        index,
                        p,
                        alternatives,
                    } => {
                        self.discard(speculative.take(), id, &at_name);
                        drop(guard);
                        (index, p, alternatives)
                    }
                    Decision::Fork { branches, fork_p } => {
                        let (first, rest) = branches.split_first().expect("a fork has branches");
                        let granted = reserve(&budget, rest.len());
                        if split && granted < rest.len() {
                            // Another branch took the budget meanwhile: give
                            // it back and escalate rather than drop a check.
                            budget.fetch_add(granted, Relaxed);
                            let reason = Escalation::SplitOverBudget;
                            frames.push(record(
                                None,
                                record::candidates(cat, std::mem::take(&mut ds)),
                                Outcome::Escalated { reason },
                            ));
                            break 'judged reason;
                        }
                        self.discard(speculative.take(), id, &at_name);
                        drop(guard);
                        let child_ids: Vec<u64> = (0..granted)
                            .map(|_| self.next_walk.fetch_add(1, Relaxed))
                            .collect();
                        let fork = Arc::new(ForkInfo {
                            record: rec_id.clone(),
                            members: std::iter::once(id)
                                .chain(child_ids.iter().copied())
                                .collect(),
                            arrow_of: std::iter::once((id, cat.arrow(frame[first.0]).name.clone()))
                                .chain(
                                    child_ids
                                        .iter()
                                        .zip(&rest[..granted])
                                        .map(|(c, (i, _))| (*c, cat.arrow(frame[*i]).name.clone())),
                                )
                                .collect(),
                        });
                        forks.push(fork);
                        let mut spawned = Vec::new();
                        for (&(index, branch_p), &child) in rest[..granted].iter().zip(&child_ids) {
                            let (mut child_path, mut child_hops, mut child_tokens) =
                                (path.clone(), hops.clone(), tokens.clone());
                            self.advance(
                                &mut child_path,
                                &mut child_hops,
                                &mut child_tokens,
                                frame[index],
                                primitive,
                                branch_p,
                            );
                            spawned.push(child);
                            if let Some(d) = ds.iter_mut().find(|d| d.arrow == frame[index]) {
                                d.disposition = Disposition::Forked {
                                    branch: Some(child),
                                };
                            }
                            let _ = branch_tx.send(Seed {
                                id: child,
                                parent: Some(id),
                                job: job.clone(),
                                path: child_path,
                                hops: child_hops,
                                depth: depth + 1,
                                budget: budget.clone(),
                                after: Some(rec_id.clone()),
                                focus: Some(self.focus(frame[index])),
                                tokens: child_tokens,
                                forks: forks.clone(),
                            });
                        }
                        for &(index, _) in &rest[granted..] {
                            if let Some(d) = ds.iter_mut().find(|d| d.arrow == frame[index]) {
                                d.disposition = Disposition::Alternative;
                                d.reason.push_str("; not pursued: branch budget exhausted");
                            }
                        }
                        // This walk continues as the first branch.
                        focus = Some(self.focus(frame[first.0]));
                        frames.push(record(
                            judge_rec.clone(),
                            record::candidates(cat, std::mem::take(&mut ds)),
                            Outcome::Forked {
                                continued: cat.arrow(frame[first.0]).name.clone(),
                                spawned: spawned.clone(),
                            },
                        ));
                        self.counters.forks.fetch_add(1, Relaxed);
                        self.counters.branches.fetch_add(granted as u64, Relaxed);
                        let name = |i: usize| cat.arrow(frame[i]).name.as_str();
                        tracing::info!(
                            target: "onto",
                            event = "fork",
                            walk = id,
                            at = %at_name,
                            fork_p = r4(fork_p),
                            branches = %branches.iter().map(|b| name(b.0)).collect::<Vec<_>>().join(","),
                            spawned = %spawned.iter().map(u64::to_string).collect::<Vec<_>>().join(","),
                        );
                        steps.push(StepRecord::Forked {
                            at: at_name.clone(),
                            branches: branches
                                .iter()
                                .map(|&(i, p)| self.alt(frame[i], p))
                                .collect(),
                            fork_p,
                            spawned,
                            wait_ms,
                        });
                        // Over budget: the remaining branches are logged
                        // as alternatives, not pursued.
                        (first.0, first.1, rest[granted..].to_vec())
                    }
                };

                // Follow `index`: a plain step, or this walk's own branch of a fork.
                for &(i, _) in &alternatives {
                    self.locks.push(Potentiality {
                        kind: PotentialityKind::Alternative,
                        resolution: Resolution::NotFollowed,
                        mode: None,
                        walk: id,
                        with: None,
                        at: at_name.clone(),
                        nodes: vec![cat.object(cat.arrow(frame[i]).dst).name.clone()],
                        wait_ms: 0.0,
                    });
                }
                let arrow = frame[index];
                let a = cat.arrow(arrow);
                if !ds.is_empty() {
                    // A plain step (a fork's record was already written).
                    frames.push(record(
                        judge_rec,
                        record::candidates(cat, ds),
                        Outcome::Followed {
                            arrow: a.name.clone(),
                            to: cat.object(a.dst).name.clone(),
                        },
                    ));
                }
                // Who, if anyone, attested what opened this arrow.
                last_attested = self.evidence(arrow, &job.case);
                if let Some(f) = frames.last_mut().filter(|f| f.id == rec_id) {
                    f.attested = last_attested.clone();
                }
                after = Some(rec_id.clone());
                tracing::info!(
                    target: "onto",
                    event = "step",
                    walk = id,
                    from = %at_name,
                    arrow = %a.name,
                    to = %cat.object(a.dst).name,
                    decided_by = primitive.as_str(),
                    p = r4(p),
                    confidence = answer.confidence().map(r4),
                    alternatives = alternatives.len(),
                );
                // Frames with memory keep this decision as a precedent.
                if let Some(_n) = cat.state_of(at).memory
                    && primitive != Primitive::Split
                {
                    let spec = cat.state_of(at);
                    self.memory.lock().unwrap().push(crate::memory::Precedent {
                        frame: at_name.clone(),
                        record: rec_id.clone(),
                        snapshot: self.snapshot.clone(),
                        case: job.case["id"].as_str().map(str::to_owned),
                        saw: crate::memory::project(&visit_seen, &spec),
                        decided: a.name.clone(),
                        p,
                    });
                }
                steps.push(StepRecord::Followed {
                    from: at_name,
                    arrow: a.name.clone(),
                    to: cat.object(a.dst).name.clone(),
                    decided_by: primitive,
                    p,
                    confidence: answer.confidence(),
                    alternatives: alternatives
                        .iter()
                        .map(|&(i, p)| self.alt(frame[i], p))
                        .collect(),
                    attested: last_attested.clone(),
                    learned: a.learned,
                    wait_ms,
                });
                self.advance(&mut path, &mut hops, &mut tokens, arrow, primitive, p);
                continue 'steps;
            };

            tracing::info!(target: "onto", event = "escalate", walk = id, at = %at_name, reason = reason.as_str());
            if reason == Escalation::SplitOverBudget {
                // Structural: a proposal cannot fix a budget; a person must.
                steps.push(StepRecord::Escalated {
                    at: at_name,
                    reason,
                    proposals: Vec::new(),
                    wait_ms,
                });
                break;
            }
            let proposals = match speculative {
                Some(h) => h.await.expect("proposer task panicked"),
                None => {
                    if mode == Mode::Read {
                        // Upgrade: System 2 may extend this frame.
                        drop(guard);
                        guard = self
                            .locks
                            .acquire(cat, Claim::new(cat, id, at, Mode::Write))
                            .await;
                    }
                    let req =
                        self.proposal_request(&job, &path, &hops, &focus, &tokens, reason.as_str());
                    self.call_proposer(id, req, false).await
                }
            };
            // Open world: admit what the supervisor lets through, then
            // re-judge this frame with it (still holding the write claim).
            let mut expanded = None;
            if let Ok(proposals) = &proposals
                && self.cfg.open_world
                && cat.learnable(at)
                && expansions < self.cfg.max_expansions
                && !proposals.is_empty()
            {
                let (learned, refused) = self.expand(&rec_id, reason, proposals).await;
                if !learned.is_empty() {
                    expansions += 1;
                    expanded = Some((learned, refused));
                }
            }
            // Record before releasing the claim, so a walk waiting on this
            // frame sees these proposals when its own proposer runs.
            if let Ok(proposals) = &proposals {
                if let Some(last) = frames.last_mut() {
                    last.proposals = proposals.clone();
                    last.seen = Some(visit_seen.clone());
                    if let Some((learned, _)) = &expanded {
                        last.outcome = Outcome::Expanded {
                            reason,
                            learned: learned.iter().map(|p| p.arrow.clone()).collect(),
                        };
                    }
                }
                self.note_concepts(id, at, proposals);
                let admitted: Vec<&str> = expanded
                    .iter()
                    .flat_map(|(l, _)| l.iter().map(|p| p.arrow.as_str()))
                    .collect();
                self.pending.lock().unwrap().entry(at).or_default().extend(
                    proposals
                        .iter()
                        .filter(|p| !admitted.contains(&p.arrow.as_str()))
                        .map(|p| (id, p.clone())),
                );
            }
            drop(guard);
            if let Some((learned, refused)) = expanded {
                steps.push(StepRecord::Expanded {
                    at: at_name,
                    reason,
                    learned,
                    refused,
                    wait_ms,
                });
                after = Some(rec_id);
                continue 'steps;
            }
            match proposals {
                Ok(proposals) => {
                    steps.push(StepRecord::Escalated {
                        at: at_name,
                        reason,
                        proposals,
                        wait_ms,
                    });
                }
                Err(e) => steps.push(self.failed(id, &at_name, e)),
            }
            break;
        }

        self.joins.ended(id);
        let report = WalkReport {
            walk: id,
            parent,
            goal: job.goal,
            from: job.from,
            path: path.display_typed(&self.cat()),
            steps,
            frames,
            elapsed_ms: ms(t0.elapsed()),
        };
        tracing::info!(
            target: "onto",
            event = "walk.end",
            walk = id,
            parent = report.parent,
            path = %report.path,
            steps = report.steps.len(),
            elapsed_ms = report.elapsed_ms,
        );
        report
    }

    /// Reviews `proposals` against the live graph and admits every one
    /// the supervisor does not reject. Structural proofs are re-run under
    /// the graph's write lock, against the graph they will extend, so two
    /// walks learning at once cannot combine into a violation.
    async fn expand(
        &self,
        record: &str,
        reason: Escalation,
        proposals: &[Proposal],
    ) -> (Vec<Proposal>, Vec<(String, String)>) {
        let (mut learned, mut refused) = (Vec::new(), Vec::new());
        let mut earlier: Vec<Proposal> = Vec::new();
        for p in proposals {
            let review = crate::supervisor::review(
                &self.cat(),
                p.arrow.clone(),
                vec![record.to_owned()],
                p,
                &earlier,
                &self.judge,
            )
            .await;
            earlier.push(p.clone());
            if review.admission == onto_core::supervise::Admission::Reject {
                let why = review
                    .checks
                    .iter()
                    .find(|c| c.outcome == onto_core::supervise::Outcome::Fail)
                    .map_or("rejected".to_owned(), |c| {
                        format!("{}: {}", c.check, c.reason)
                    });
                refused.push((p.arrow.clone(), why));
                continue;
            }
            let mut graph = self.graph.write().unwrap();
            let (next, skipped) = (**graph).clone().with_learned(std::slice::from_ref(p));
            if let Some((_, why)) = skipped.into_iter().next() {
                refused.push((p.arrow.clone(), why));
                continue;
            }
            *graph = Arc::new(next);
            drop(graph);
            tracing::info!(
                target: "onto",
                event = "learned",
                record,
                arrow = %p.arrow,
                from = %p.src,
                to = %p.dst,
                admission = ?review.admission,
            );
            self.learned.lock().unwrap().push(Learned {
                proposal: p.clone(),
                record: record.to_owned(),
                reason: reason.as_str().to_owned(),
                snapshot: self.snapshot.clone(),
                checks: review
                    .checks
                    .iter()
                    .map(|c| format!("{}: {:?}", c.check, c.outcome).to_lowercase())
                    .collect(),
            });
            learned.push(p.clone());
        }
        (learned, refused)
    }

    fn advance(
        &self,
        path: &mut Path,
        hops: &mut Vec<Hop>,
        tokens: &mut BTreeSet<String>,
        arrow: ArrowId,
        decided_by: Primitive,
        p: f32,
    ) {
        let cat_arc = self.cat();
        let cat = &*cat_arc;
        let a = cat.arrow(arrow);
        hops.push(Hop {
            from: cat.object(a.src).name.clone(),
            arrow: a.name.clone(),
            to: cat.object(a.dst).name.clone(),
            decided_by,
            p,
        });
        *tokens = a.effect(tokens);
        path.push(cat, arrow)
            .expect("frame arrows leave the current object");
    }

    /// The verified observations behind an attested arrow, as
    /// `Attester: field, … @ time` (empty if the arrow needs none).
    fn evidence(&self, arrow: ArrowId, case: &Value) -> Vec<String> {
        let cat = self.cat();
        let Some(need) = &cat.arrow(arrow).attested else {
            return Vec::new();
        };
        let paths = need.paths();
        let (_, ok, _) = onto_core::attest::attested_view(self.cat().attesters(), case);
        ok.into_iter()
            .filter(|v| v.fields.iter().any(|f| paths.contains(f)))
            .map(|v| format!("{}: {} @ {}", v.attester, v.fields.join(", "), v.at))
            .collect()
    }

    fn alt(&self, arrow: ArrowId, p: f32) -> Alt {
        let cat = self.cat();
        let a = cat.arrow(arrow);
        Alt {
            arrow: a.name.clone(),
            to: self.cat().object(a.dst).name.clone(),
            p,
        }
    }

    fn discard(
        &self,
        speculative: Option<tokio::task::JoinHandle<Result<Vec<Proposal>, ModelError>>>,
        walk: u64,
        at: &str,
    ) {
        if let Some(h) = speculative {
            h.abort();
            self.counters.speculative_discarded.fetch_add(1, Relaxed);
            tracing::info!(target: "onto", event = "proposer.discarded", walk, at);
        }
    }

    async fn call_judge(
        &self,
        walk: u64,
        req: FrameRequest,
    ) -> Result<(Answer, JudgeRecord), ModelError> {
        let _slot = self.judge_slots.acquire().await.expect("semaphore open");
        let at = req.at.clone();
        let primitive = req.primitive;
        let t0 = Instant::now();
        let result = self.judge.judge(req).await;
        let latency = t0.elapsed();
        self.count(latency, result.as_ref().ok().map(|r| r.1));
        self.counters.judge_calls.fetch_add(1, Relaxed);
        match &result {
            Ok((a, u)) => {
                self.counters
                    .judge_questions
                    .fetch_add(u64::from(u.questions), Relaxed);
                let (top_p, holds, fork_p) = match a {
                    Answer::Choice(d) => (d.top().map(|t| r4(t.1)), None, None),
                    Answer::Noul { holds, fork } => (
                        None,
                        Some(
                            holds
                                .iter()
                                .map(|p| r4(*p).to_string())
                                .collect::<Vec<_>>()
                                .join(","),
                        ),
                        fork.map(r4),
                    ),
                    Answer::Score { levels, .. } => (
                        levels
                            .iter()
                            .copied()
                            .fold(None, |m: Option<f32>, p| Some(m.map_or(p, |m| m.max(p))))
                            .map(r4),
                        None,
                        None,
                    ),
                };
                tracing::info!(
                    target: "onto",
                    event = "judge.call",
                    walk,
                    at = %at,
                    primitive = primitive.as_str(),
                    questions = u.questions,
                    latency_ms = ms(latency),
                    top_p,
                    holds,
                    fork_p,
                    confidence = a.confidence().map(r4),
                    input_tokens = u.input_tokens,
                    output_tokens = u.output_tokens,
                    attempts = u.attempts,
                    ok = true,
                );
            }
            Err(e) => tracing::warn!(
                target: "onto",
                event = "judge.call",
                walk,
                at = %at,
                primitive = primitive.as_str(),
                latency_ms = ms(latency),
                ok = false,
                error = %e,
            ),
        }
        result.map(|(answer, u)| {
            let rec = JudgeRecord {
                model: Judge::name(&self.judge),
                latency_ms: ms(latency),
                questions: u.questions,
                confidence: answer.confidence(),
                none_of_these: match &answer {
                    Answer::Choice(d) => Some(d.none_of_these),
                    _ => None,
                },
                fork_p: match &answer {
                    Answer::Noul { fork, .. } => *fork,
                    _ => None,
                },
                input_tokens: u.input_tokens,
                output_tokens: u.output_tokens,
            };
            (answer, rec)
        })
    }

    async fn call_proposer(
        &self,
        walk: u64,
        req: ProposalRequest,
        speculative: bool,
    ) -> Result<Vec<Proposal>, ModelError> {
        let _slot = self.proposer_slots.acquire().await.expect("semaphore open");
        let at = req.at.clone();
        let pending: Vec<(String, String)> = req
            .pending_here
            .iter()
            .map(|p| (p.arrow.clone(), p.target.clone()))
            .collect();
        let t0 = Instant::now();
        let result = self.proposer.propose(req).await;
        let latency = t0.elapsed();
        self.count(latency, result.as_ref().ok().map(|r| r.1));
        self.counters.proposer_calls.fetch_add(1, Relaxed);
        match &result {
            Ok((p, u)) => tracing::info!(
                target: "onto",
                event = "proposer.call",
                walk,
                at = %at,
                speculative,
                latency_ms = ms(latency),
                proposals = p.len(),
                pending_here = pending.len(),
                reused = p
                    .iter()
                    .filter(|x| pending.contains(&(x.arrow.clone(), x.dst.clone())))
                    .count(),
                input_tokens = u.input_tokens,
                output_tokens = u.output_tokens,
                attempts = u.attempts,
                ok = true,
            ),
            Err(e) => tracing::warn!(
                target: "onto",
                event = "proposer.call",
                walk,
                at = %at,
                speculative,
                latency_ms = ms(latency),
                ok = false,
                error = %e,
            ),
        }
        result.map(|r| r.0)
    }

    fn count(&self, latency: Duration, usage: Option<Usage>) {
        let c = &self.counters;
        c.model_us.fetch_add(latency.as_micros() as u64, Relaxed);
        if let Some(u) = usage {
            c.tokens_in.fetch_add(u.input_tokens, Relaxed);
            c.tokens_out.fetch_add(u.output_tokens, Relaxed);
        }
    }

    /// Records a conceptual potentiality when another walk has already
    /// proposed the same new concept (or the same arrow into an existing one).
    fn note_concepts(&self, walk: u64, at: ObjId, proposals: &[Proposal]) {
        let cat_arc = self.cat();
        let cat = &*cat_arc;
        for p in proposals {
            let key = match cat.object_id(&p.dst) {
                Ok(_) => format!("{}->{}", normalize(&p.src), normalize(&p.dst)),
                Err(_) => normalize(&p.dst),
            };
            let others = {
                let mut concepts = self.concepts.lock().unwrap();
                let walks = concepts.entry(key).or_default();
                let others: Vec<u64> = walks.iter().copied().filter(|w| *w != walk).collect();
                walks.push(walk);
                others
            };
            for with in others {
                self.locks.push(Potentiality {
                    kind: PotentialityKind::Conceptual,
                    resolution: Resolution::Coexisted,
                    mode: None,
                    walk,
                    with: Some(with),
                    at: cat.object(at).name.clone(),
                    nodes: vec![p.dst.clone()],
                    wait_ms: 0.0,
                });
            }
        }
    }

    fn failed(&self, walk: u64, at: &str, e: ModelError) -> StepRecord {
        tracing::warn!(target: "onto", event = "walk.failed", walk, at, error = %e);
        StepRecord::Failed {
            at: at.to_owned(),
            error: e.to_string(),
        }
    }

    fn candidate(&self, arrow: ArrowId) -> Candidate {
        Candidate::of(&self.cat(), arrow)
    }

    fn focus(&self, arrow: ArrowId) -> Focus {
        let cat = self.cat();
        let a = cat.arrow(arrow);
        Focus {
            arrow: a.name.clone(),
            to: self.cat().object(a.dst).name.clone(),
            condition: a.instructions.clone(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn frame_request(
        &self,
        job: &Job,
        path: &Path,
        hops: &[Hop],
        focus: &Option<Focus>,
        tokens: &BTreeSet<String>,
        frame: &[ArrowId],
        can_fork: bool,
    ) -> FrameRequest {
        let cat = self.cat();
        let object = cat.object(path.dst);
        FrameRequest {
            state: self.state(&cat, job, path, hops, focus, tokens),
            at: object.name.clone(),
            focus: focus.clone(),
            primitive: object.frame.primitive,
            instructions: object.frame.instructions.clone(),
            candidates: frame.iter().map(|a| self.candidate(*a)).collect(),
            can_fork,
            parallel: object.frame.parallel,
        }
    }

    /// The state a model sees at the frame `path` ends at: exactly what
    /// the frame's `state` declaration allows. Without any declaration in
    /// the category, the shape every frame always had.
    fn state(
        &self,
        cat: &Category,
        job: &Job,
        path: &Path,
        hops: &[Hop],
        focus: &Option<Focus>,
        tokens: &BTreeSet<String>,
    ) -> Value {
        use serde_json::{Map, json};
        let object = cat.object(path.dst);
        if !cat.declares_state() {
            return json!({
                "goal": job.goal,
                "case": job.case,
                "at": object.name,
                "about_at": object.about,
                "path_so_far": path.display(cat),
                "hops": hops,
                "focus": focus,
                "tokens_held": tokens,
            });
        }
        let spec = cat.state_of(path.dst);
        let mut s = Map::new();
        s.insert("at".into(), json!(object.name));
        if let Some(about) = &object.about {
            s.insert("about_at".into(), about.clone());
        }
        if spec.goal {
            s.insert("goal".into(), json!(job.goal));
        }
        // Asserted by the submitter (not verified).
        let asserted = match &spec.case {
            onto_core::CaseView::Hidden => None,
            onto_core::CaseView::Whole => {
                let mut c = job.case.clone();
                if let Some(m) = c.as_object_mut() {
                    // Raw observations are shown only verified (`observed`).
                    m.remove("observations");
                }
                Some(c)
            }
            onto_core::CaseView::Fields(fields) => {
                let mut picked = Value::Object(Map::new());
                for f in fields {
                    if let Some(v) = f.split('.').try_fold(&job.case, |v, k| v.get(k)) {
                        insert_path(&mut picked, f, v.clone());
                    }
                }
                Some(picked)
            }
        };
        if let Some(a) = asserted {
            s.insert("asserted".into(), a);
        }
        // Verified: signed by declared attesters, bound to this case.
        if spec.observed {
            let (view, _, _) = onto_core::attest::attested_view(cat.attesters(), &job.case);
            s.insert("observed".into(), view);
        }
        // Inferred: earlier judgments of this walk.
        if let Some(n) = spec.history {
            let recent = &hops[hops.len().saturating_sub(n)..];
            let mut inferred = Map::new();
            if n == usize::MAX {
                inferred.insert("path_so_far".into(), json!(path.display(cat)));
            }
            inferred.insert("hops".into(), json!(recent));
            s.insert("inferred".into(), Value::Object(inferred));
        }
        if let Some(n) = spec.memory {
            // Precedents carry only what this frame may see (projected).
            let now = crate::memory::project(&Value::Object(s.clone()), &spec);
            let store = self.memory.lock().unwrap();
            let found = crate::memory::similar(
                &store[..self.memory_loaded.load(Relaxed).min(store.len())],
                &object.name,
                job.case["id"].as_str(),
                &now,
                n,
            );
            if !found.is_empty() {
                let shown: Vec<Value> = found
                    .iter()
                    .map(|p| {
                        json!({
                            "saw": crate::memory::project(&p.saw, &spec),
                            "decided": p.decided,
                            "to": cat.arrow_id(&p.decided).ok().map(|a| cat.object(cat.arrow(a).dst).name.clone()),
                            "p": r4(p.p),
                        })
                    })
                    .collect();
                s.insert(
                    "precedents".into(),
                    json!({
                        "note": "earlier decisions at this frame for other cases (not evidence about this case)",
                        "cases": shown,
                    }),
                );
            }
        }
        if spec.focus && focus.is_some() {
            s.insert("focus".into(), json!(focus));
        }
        if spec.tokens {
            s.insert("tokens_held".into(), json!(tokens));
        }
        Value::Object(s)
    }

    fn proposal_request(
        &self,
        job: &Job,
        path: &Path,
        hops: &[Hop],
        focus: &Option<Focus>,
        tokens: &BTreeSet<String>,
        reason: &str,
    ) -> ProposalRequest {
        let cat = self.cat();
        let object = cat.object(path.dst);
        ProposalRequest {
            state: self.state(&cat, job, path, hops, focus, tokens),
            at: object.name.clone(),
            about_at: object.about.clone(),
            path_so_far: path.display(&self.cat()),
            focus: focus.clone(),
            primitive: object.frame.primitive,
            frame: cat
                .out(path.dst)
                .iter()
                .map(|a| self.candidate(*a))
                .collect(),
            reason: reason.to_owned(),
            known_objects: cat.objects().iter().map(|o| o.name.clone()).collect(),
            outcomes: (0..cat.objects().len() as u32)
                .map(ObjId)
                .filter(|o| cat.object(*o).closure == Closure::Closed && cat.out(*o).is_empty())
                .map(|o| {
                    let reached_by: Vec<String> = (0..cat.arrows().len() as u32)
                        .map(ArrowId)
                        .map(|a| cat.arrow(a))
                        .filter(|a| a.dst == o && !a.learned)
                        .take(6)
                        .map(|a| {
                            let about = a.instructions.as_ref().map_or(String::new(), |i| {
                                format!(" ({})", i.as_str().map_or(i.to_string(), str::to_owned))
                            });
                            format!(
                                "{}: {} -> {}{about}",
                                a.name,
                                cat.object(a.src).name,
                                cat.object(o).name
                            )
                        })
                        .collect();
                    serde_json::json!({"object": cat.object(o).name, "reached_by": reached_by})
                })
                .collect(),
            pending_here: self
                .pending
                .lock()
                .unwrap()
                .get(&path.dst)
                .map(|ps| {
                    ps.iter()
                        .map(|(walk, p)| Pending {
                            walk: *walk,
                            arrow: p.arrow.clone(),
                            target: p.dst.clone(),
                            about: p.about.clone(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
        }
    }
}

/// Takes up to `want` from the shared branch budget; returns how many.
fn reserve(budget: &AtomicUsize, want: usize) -> usize {
    let mut granted = 0;
    let _ = budget.fetch_update(Relaxed, Relaxed, |left| {
        granted = left.min(want);
        Some(left - granted)
    });
    granted
}

/// f32 probabilities as clean decimals in logs (0.95, not 0.949999988).
fn r4(x: f32) -> f64 {
    (f64::from(x) * 1e4).round() / 1e4
}

fn ms(d: Duration) -> f64 {
    (d.as_secs_f64() * 1e6).round() / 1e3
}

/// Case- and separator-insensitive concept key: `Password_Reset` = `passwordreset`.
fn normalize(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Sets `a.b.c` in nested JSON objects.
pub(crate) fn insert_path(root: &mut Value, dotted: &str, value: Value) {
    let mut cursor = root;
    let parts: Vec<&str> = dotted.split('.').collect();
    for (i, p) in parts.iter().enumerate() {
        let Some(map) = cursor.as_object_mut() else {
            return;
        };
        if i + 1 == parts.len() {
            map.insert((*p).to_owned(), value);
            return;
        }
        cursor = map
            .entry((*p).to_owned())
            .or_insert_with(|| Value::Object(serde_json::Map::new()));
    }
}
