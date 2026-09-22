//! The core loop: many walks at once, each step claiming its frame,
//! System-1 calls first, System-2 calls on escalation.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use onto_core::category::Closure;
use onto_core::walk::{Decision, Escalation, Proposal, decide};
use onto_core::{Category, ObjId, Path};
use serde::Serialize;
use tokio::sync::Semaphore;

use crate::frames::{Claim, FrameLocks, Mode, Policy, Potentiality, PotentialityKind, Resolution};
use crate::mem::{self, MemSample};
use crate::model::{
    Candidate, ChoiceRequest, Chooser, ModelError, ProposalRequest, Proposer, Usage,
};

#[derive(Clone, Debug)]
pub struct Job {
    pub goal: String,
    pub from: String,
}

#[derive(Clone, Debug)]
pub struct Config {
    /// Minimum System-1 confidence to follow an arrow without escalating.
    pub threshold: f32,
    pub policy: Policy,
    /// Start the System-2 call alongside System 1 at every closed frame and
    /// cancel it if System 1 is confident. Lower latency, more tokens.
    pub speculate: bool,
    pub max_steps: usize,
    /// In-flight call limits per provider.
    pub chooser_concurrency: usize,
    pub proposer_concurrency: usize,
    pub mem_sample_every: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            threshold: 0.6,
            policy: Policy::default(),
            speculate: false,
            max_steps: 16,
            chooser_concurrency: 16,
            proposer_concurrency: 4,
            mem_sample_every: Duration::from_millis(250),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "step", rename_all = "snake_case")]
pub enum StepRecord {
    Followed {
        from: String,
        arrow: String,
        to: String,
        p: f32,
        confidence: Option<f32>,
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
}

#[derive(Clone, Debug, Serialize)]
pub struct WalkReport {
    pub walk: u64,
    pub goal: String,
    pub from: String,
    pub path: String,
    pub steps: Vec<StepRecord>,
    pub elapsed_ms: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct RunReport {
    pub chooser: String,
    pub proposer: String,
    pub policy: Policy,
    pub speculate: bool,
    pub walks: Vec<WalkReport>,
    pub potentialities: Vec<Potentiality>,
    pub wall_ms: f64,
    /// Sum of every model call's latency; `model_ms_sum / wall_ms` is the
    /// effective parallelism.
    pub model_ms_sum: f64,
    pub chooser_calls: u64,
    pub proposer_calls: u64,
    pub speculative_discarded: u64,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub mem_start: MemSample,
    pub mem_end: MemSample,
    pub peak_rss_bytes: Option<usize>,
}

#[derive(Default)]
struct Counters {
    model_us: AtomicU64,
    chooser_calls: AtomicU64,
    proposer_calls: AtomicU64,
    speculative_discarded: AtomicU64,
    tokens_in: AtomicU64,
    tokens_out: AtomicU64,
}

pub struct Engine<C, P> {
    cat: Arc<Category>,
    chooser: C,
    proposer: P,
    cfg: Config,
    locks: FrameLocks,
    chooser_slots: Semaphore,
    proposer_slots: Semaphore,
    /// Proposed concept key -> walks that proposed it.
    concepts: Mutex<HashMap<String, Vec<u64>>>,
    counters: Counters,
}

impl<C: Chooser, P: Proposer> Engine<C, P> {
    pub fn new(cat: Arc<Category>, chooser: C, proposer: P, cfg: Config) -> Arc<Self> {
        Arc::new(Self {
            locks: FrameLocks::new(cfg.policy),
            chooser_slots: Semaphore::new(cfg.chooser_concurrency),
            proposer_slots: Semaphore::new(cfg.proposer_concurrency),
            concepts: Mutex::new(HashMap::new()),
            counters: Counters::default(),
            cat,
            chooser,
            proposer,
            cfg,
        })
    }

    /// Runs every job concurrently and reports once all have finished.
    pub async fn run(self: &Arc<Self>, jobs: Vec<Job>) -> Result<RunReport, onto_core::Error> {
        let starts = jobs
            .iter()
            .map(|j| self.cat.object_id(&j.from))
            .collect::<Result<Vec<_>, _>>()?;
        let mem_start = mem::sample();
        let sampler = mem::spawn_sampler(self.cfg.mem_sample_every);
        tracing::info!(
            target: "onto",
            event = "run.start",
            category = self.cat.name(),
            walks = jobs.len(),
            chooser = %self.chooser.name(),
            proposer = %self.proposer.name(),
            policy = self.cfg.policy.as_str(),
            speculate = self.cfg.speculate,
            threshold = r4(self.cfg.threshold),
        );
        let t0 = Instant::now();
        let handles: Vec<_> = jobs
            .into_iter()
            .zip(starts)
            .enumerate()
            .map(|(i, (job, from))| {
                let engine = self.clone();
                tokio::spawn(async move { engine.walk(i as u64 + 1, job, from).await })
            })
            .collect();
        let mut walks = Vec::with_capacity(handles.len());
        for h in handles {
            walks.push(h.await.expect("walk task panicked"));
        }
        let wall = t0.elapsed();
        let mem_end = mem::sample();
        let peak_rss_bytes = sampler.stop().max(mem_end.rss_bytes);
        let c = &self.counters;
        let report = RunReport {
            chooser: self.chooser.name(),
            proposer: self.proposer.name(),
            policy: self.cfg.policy,
            speculate: self.cfg.speculate,
            walks,
            potentialities: self.locks.potentialities(),
            wall_ms: ms(wall),
            model_ms_sum: c.model_us.load(Relaxed) as f64 / 1e3,
            chooser_calls: c.chooser_calls.load(Relaxed),
            proposer_calls: c.proposer_calls.load(Relaxed),
            speculative_discarded: c.speculative_discarded.load(Relaxed),
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
            chooser_calls = report.chooser_calls,
            proposer_calls = report.proposer_calls,
            speculative_discarded = report.speculative_discarded,
            potentialities = report.potentialities.len(),
            tokens_in = report.tokens_in,
            tokens_out = report.tokens_out,
            peak_rss_bytes = report.peak_rss_bytes,
            heap_peak_bytes = report.mem_end.heap_peak_bytes,
        );
        Ok(report)
    }

    async fn walk(self: Arc<Self>, id: u64, job: Job, from: ObjId) -> WalkReport {
        let cat = &*self.cat;
        let t0 = Instant::now();
        tracing::info!(target: "onto", event = "walk.start", walk = id, from = %job.from, goal = %job.goal);
        let mut path = Path::id(from);
        let mut steps = Vec::new();

        for _ in 0..self.cfg.max_steps {
            let at = path.dst;
            let closed = cat.object(at).closure == Closure::Closed;
            let frame = cat.out(at);
            if closed && frame.is_empty() {
                break; // terminal
            }
            let at_name = cat.object(at).name.clone();

            // Claim the frame. With speculation the System-2 call may start
            // right away, so the claim is a write from the start.
            let mode = if self.cfg.speculate || !closed {
                Mode::Write
            } else {
                Mode::Read
            };
            let mut guard = self.locks.acquire(cat, Claim::new(cat, id, at, mode)).await;
            let wait_ms = ms(guard.waited);

            let mut speculative = None;
            let reason = if closed {
                if self.cfg.speculate {
                    let engine = self.clone();
                    let req = self.proposal_request(
                        &job,
                        &path,
                        "speculative: started alongside System 1",
                    );
                    speculative = Some(tokio::spawn(async move {
                        engine.call_proposer(id, req, true).await
                    }));
                }
                let d = match self
                    .call_chooser(id, self.choice_request(&job, &path))
                    .await
                {
                    Ok(d) => d,
                    Err(e) => {
                        if let Some(h) = speculative {
                            h.abort();
                        }
                        steps.push(self.failed(id, &at_name, e));
                        break;
                    }
                };
                match decide(Closure::Closed, Some(&d), self.cfg.threshold) {
                    Decision::Follow { index, p } => {
                        if let Some(h) = speculative {
                            h.abort();
                            self.counters.speculative_discarded.fetch_add(1, Relaxed);
                            tracing::info!(target: "onto", event = "proposer.discarded", walk = id, at = %at_name);
                        }
                        drop(guard);
                        let arrow = frame[index];
                        path.push(cat, arrow)
                            .expect("frame arrows leave the current object");
                        let a = cat.arrow(arrow);
                        tracing::info!(
                            target: "onto",
                            event = "step",
                            walk = id,
                            from = %at_name,
                            arrow = %a.name,
                            to = %cat.object(a.dst).name,
                            p = r4(p),
                            confidence = d.confidence.map(r4),
                        );
                        steps.push(StepRecord::Followed {
                            from: at_name,
                            arrow: a.name.clone(),
                            to: cat.object(a.dst).name.clone(),
                            p,
                            confidence: d.confidence,
                            wait_ms,
                        });
                        continue;
                    }
                    Decision::Escalate(reason) => reason,
                }
            } else {
                Escalation::OpenFrame
            };

            tracing::info!(target: "onto", event = "escalate", walk = id, at = %at_name, reason = escalation_str(reason));
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
                    let req = self.proposal_request(&job, &path, escalation_str(reason));
                    self.call_proposer(id, req, false).await
                }
            };
            drop(guard);
            match proposals {
                Ok(proposals) => {
                    self.note_concepts(id, at, &proposals);
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

        let report = WalkReport {
            walk: id,
            goal: job.goal,
            from: job.from,
            path: path.display_typed(cat),
            steps,
            elapsed_ms: ms(t0.elapsed()),
        };
        tracing::info!(
            target: "onto",
            event = "walk.end",
            walk = id,
            path = %report.path,
            steps = report.steps.len(),
            elapsed_ms = report.elapsed_ms,
        );
        report
    }

    async fn call_chooser(
        &self,
        walk: u64,
        req: ChoiceRequest,
    ) -> Result<onto_core::walk::Distribution, ModelError> {
        let _slot = self.chooser_slots.acquire().await.expect("semaphore open");
        let at = req.at.clone();
        let t0 = Instant::now();
        let result = self.chooser.choose(req).await;
        let latency = t0.elapsed();
        self.count(latency, result.as_ref().ok().map(|r| r.1));
        self.counters.chooser_calls.fetch_add(1, Relaxed);
        match &result {
            Ok((d, u)) => tracing::info!(
                target: "onto",
                event = "chooser.call",
                walk,
                at = %at,
                latency_ms = ms(latency),
                top_p = d.top().map(|t| r4(t.1)),
                none_of_these = r4(d.none_of_these),
                confidence = d.confidence.map(r4),
                input_tokens = u.input_tokens,
                output_tokens = u.output_tokens,
                attempts = u.attempts,
                ok = true,
            ),
            Err(e) => tracing::warn!(
                target: "onto",
                event = "chooser.call",
                walk,
                at = %at,
                latency_ms = ms(latency),
                ok = false,
                error = %e,
            ),
        }
        result.map(|r| r.0)
    }

    async fn call_proposer(
        &self,
        walk: u64,
        req: ProposalRequest,
        speculative: bool,
    ) -> Result<Vec<Proposal>, ModelError> {
        let _slot = self.proposer_slots.acquire().await.expect("semaphore open");
        let at = req.at.clone();
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
        let cat = &*self.cat;
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
                    with,
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

    fn frame(&self, at: ObjId) -> Vec<Candidate> {
        let cat = &*self.cat;
        cat.out(at)
            .iter()
            .map(|a| {
                let a = cat.arrow(*a);
                Candidate {
                    arrow: a.name.clone(),
                    to: cat.object(a.dst).name.clone(),
                }
            })
            .collect()
    }

    fn choice_request(&self, job: &Job, path: &Path) -> ChoiceRequest {
        ChoiceRequest {
            goal: job.goal.clone(),
            at: self.cat.object(path.dst).name.clone(),
            path_so_far: path.display(&self.cat),
            frame: self.frame(path.dst),
        }
    }

    fn proposal_request(&self, job: &Job, path: &Path, reason: &str) -> ProposalRequest {
        ProposalRequest {
            goal: job.goal.clone(),
            at: self.cat.object(path.dst).name.clone(),
            path_so_far: path.display(&self.cat),
            frame: self.frame(path.dst),
            reason: reason.to_owned(),
            known_objects: self.cat.objects().iter().map(|o| o.name.clone()).collect(),
        }
    }
}

fn escalation_str(e: Escalation) -> &'static str {
    match e {
        Escalation::OpenFrame => "open_frame",
        Escalation::NoneOfThese => "none_of_these",
        Escalation::LowConfidence => "low_confidence",
    }
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
