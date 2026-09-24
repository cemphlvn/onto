//! `onto run`: the concurrent core loop from the command line.

use std::path::PathBuf;
use std::time::Duration;

use clap::{Args, ValueEnum};
use onto_core::walk::{Answer, Proposal};
use onto_remote::{Jev, OpenRouter};
use onto_runtime::engine::StepRecord;
use onto_runtime::frames::{Mode, PotentialityKind, Resolution};
use onto_runtime::model::{
    Critic, FrameRequest, Judge, MockJudge, MockProposer, ModelError, NoulQuestion,
    ProposalRequest, Proposer, Usage,
};
use onto_runtime::{Config, Engine, Job, Policy, RunReport, telemetry};
use serde_json::{Value, json};

#[derive(Args)]
pub struct RunArgs {
    file: PathBuf,
    /// Jobs file: one `StartObject: goal` per line.
    #[arg(long)]
    jobs: PathBuf,
    /// Use offline mock models for both tiers.
    #[arg(long)]
    mock: bool,
    /// Use the mock proposer only (live Jev, no OpenRouter spend).
    #[arg(long)]
    mock_proposer: bool,
    #[arg(long, value_enum, default_value_t = PolicyArg::Shared)]
    policy: PolicyArg,
    /// Start System 2 alongside System 1 and cancel it when not needed.
    #[arg(long)]
    speculate: bool,
    #[arg(long, default_value_t = 0.6)]
    threshold: f32,
    #[arg(long, default_value_t = 16)]
    max_steps: usize,
    /// Write telemetry as JSON lines to this file.
    #[arg(long)]
    telemetry: Option<PathBuf>,
    /// Write one disposition record per frame visit (JSON lines).
    #[arg(long)]
    dispositions: Option<PathBuf>,
    /// Write the full run report as JSON to this file.
    #[arg(long)]
    report: Option<PathBuf>,
    /// Extra branches one job may fork into (noul frames).
    #[arg(long, default_value_t = 4)]
    max_branches: usize,
    #[arg(long)]
    judge_model: Option<String>,
    #[arg(long)]
    proposer_model: Option<String>,
    #[command(flatten)]
    world: crate::learned::WorldArgs,
    /// Also show each walk's image under a functor from this category:
    /// `FILE#Functor` (or `FILE` when it declares one functor).
    #[arg(long)]
    view: Vec<PathBuf>,
    /// Start job i after i × this many milliseconds (an arrival stream).
    #[arg(long, default_value_t = 0)]
    stagger: u64,
    /// Compute review proposals on the case's path (sealed frames,
    /// closed-world runs) instead of writing gap signals for curation.
    #[arg(long)]
    propose_inline: bool,
}

#[derive(Clone, Copy, ValueEnum)]
enum PolicyArg {
    Exclusive,
    Shared,
}

pub enum AnyJudge {
    Jev(Jev),
    Mock(MockJudge),
}

impl Judge for AnyJudge {
    fn name(&self) -> String {
        match self {
            Self::Jev(j) => Judge::name(j),
            Self::Mock(j) => Judge::name(j),
        }
    }
    async fn judge(&self, req: FrameRequest) -> Result<(Answer, Usage), ModelError> {
        match self {
            Self::Jev(j) => j.judge(req).await,
            Self::Mock(j) => j.judge(req).await,
        }
    }
}

/// The judge also critiques open-world proposals before they are learned.
impl Critic for AnyJudge {
    fn name(&self) -> String {
        match self {
            Self::Jev(j) => Critic::name(j),
            Self::Mock(j) => Critic::name(j),
        }
    }
    async fn nouls(
        &self,
        state: Value,
        questions: Vec<NoulQuestion>,
    ) -> Result<(Vec<f32>, Usage), ModelError> {
        match self {
            Self::Jev(j) => j.nouls(state, questions).await,
            Self::Mock(j) => j.nouls(state, questions).await,
        }
    }
}

pub enum AnyProposer {
    OpenRouter(OpenRouter),
    Mock(MockProposer),
}

impl Proposer for AnyProposer {
    fn name(&self) -> String {
        match self {
            Self::OpenRouter(p) => p.name(),
            Self::Mock(p) => p.name(),
        }
    }
    async fn propose(&self, req: ProposalRequest) -> Result<(Vec<Proposal>, Usage), ModelError> {
        match self {
            Self::OpenRouter(p) => p.propose(req).await,
            Self::Mock(p) => p.propose(req).await,
        }
    }
}

pub type BoxError = Box<dyn std::error::Error>;

pub fn main(args: RunArgs) -> Result<(), BoxError> {
    let cat = args.world.load(&args.file)?;
    let jobs = read_jobs(&args.jobs)?;
    if let Some(path) = &args.telemetry {
        telemetry::init_jsonl(path)?;
    }

    let (judge, proposer) = models(
        args.mock,
        args.mock_proposer,
        args.judge_model.clone(),
        args.proposer_model.clone(),
    )?;
    let cfg = Config {
        threshold: args.threshold,
        policy: match args.policy {
            PolicyArg::Exclusive => Policy::Exclusive,
            PolicyArg::Shared => Policy::Shared,
        },
        speculate: args.speculate,
        max_steps: args.max_steps,
        max_branches: args.max_branches,
        open_world: !args.world.closed_world,
        max_expansions: args.world.max_expansions,
        assured: args.world.assured(),
        stagger: Duration::from_millis(args.stagger),
        review_inline: args.propose_inline,
        ..Config::default()
    };

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let precedents = args.world.precedents(&args.file, &cat)?;
    let (lenses, entries) = args.world.lenses(&args.file, &cat)?;
    let engine = Engine::new(cat, judge, proposer, cfg);
    engine.remember(precedents);
    engine.use_lenses(lenses, &entries);
    engine.use_library(entries);
    let report = rt.block_on(engine.run(jobs))?;

    print_summary(&report, args.telemetry.as_deref());
    for v in &args.view {
        print_view(v, &report)?;
    }
    args.world.save(&args.file, &report.learned)?;
    args.world.save_recalled(&args.file, &report.recalled)?;
    args.world.save_gaps(&args.file, &report.gaps)?;
    args.world.save_precedents(&args.file, &report.precedents)?;
    if let Some(path) = &args.dispositions {
        let n = write_dispositions(&report, path)?;
        println!("dispositions: {} ({n} frame records)", path.display());
    }
    if let Some(path) = &args.report {
        std::fs::write(path, serde_json::to_string_pretty(&report)?)?;
        println!("report: {}", path.display());
    }
    Ok(())
}

/// Live Jev + OpenRouter, or mocks.
pub fn models(
    mock: bool,
    mock_proposer: bool,
    judge_model: Option<String>,
    proposer_model: Option<String>,
) -> Result<(AnyJudge, AnyProposer), BoxError> {
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()?;
    let judge = if mock {
        AnyJudge::Mock(MockJudge {
            latency: Duration::from_millis(150),
        })
    } else {
        AnyJudge::Jev(
            Jev::from_env(http.clone(), judge_model)
                .ok_or("TYPESAFE_API_KEY is not set (or pass --mock)")?,
        )
    };
    let proposer = if mock || mock_proposer {
        AnyProposer::Mock(MockProposer {
            latency: Duration::from_millis(600),
        })
    } else {
        AnyProposer::OpenRouter(
            OpenRouter::from_env(http, proposer_model)
                .ok_or("OPENROUTER_API_KEY is not set (or pass --mock-proposer)")?,
        )
    };
    Ok((judge, proposer))
}

/// Every frame record of the run, walks in id order, as JSON lines.
pub fn write_dispositions(report: &RunReport, path: &std::path::Path) -> Result<usize, BoxError> {
    let mut walks: Vec<_> = report.walks.iter().collect();
    walks.sort_by_key(|w| w.walk);
    let mut out = String::new();
    let mut n = 0;
    for r in walks.iter().flat_map(|w| &w.frames) {
        out.push_str(&serde_json::to_string(r)?);
        out.push('\n');
        n += 1;
    }
    std::fs::write(path, out)?;
    Ok(n)
}

fn read_jobs(path: &PathBuf) -> Result<Vec<Job>, BoxError> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut jobs = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (from, goal) = line
            .split_once(':')
            .ok_or_else(|| format!("{}:{}: expected `StartObject: goal`", path.display(), i + 1))?;
        let (goal, case) =
            case(goal.trim()).map_err(|e| format!("{}:{}: {e}", path.display(), i + 1))?;
        jobs.push(Job {
            from: from.trim().to_owned(),
            goal,
            case,
        });
    }
    Ok(jobs)
}

/// A case is plain text, or a JSON object with a `goal` string plus any
/// structured facts (read by `require` clauses).
pub fn case(text: &str) -> Result<(String, Value), String> {
    if !text.starts_with('{') {
        return Ok((text.to_owned(), json!({})));
    }
    let v: Value = serde_json::from_str(text).map_err(|e| format!("invalid case JSON: {e}"))?;
    let goal = v["goal"]
        .as_str()
        .ok_or("case JSON needs a \"goal\" string")?
        .to_owned();
    Ok((goal, v))
}

fn print_summary(r: &RunReport, telemetry: Option<&std::path::Path>) {
    println!(
        "run: {} walks ({} branches) · judge {} · proposer {} · policy {} · speculate {}",
        r.walks.len(),
        r.branches,
        r.judge,
        r.proposer,
        r.policy.as_str(),
        if r.speculate { "on" } else { "off" }
    );
    println!();
    for w in &r.walks {
        let branch = w
            .parent
            .map_or(String::new(), |p| format!(" (branch of walk {p})"));
        println!("walk {}{branch} [{:.0}ms] {}", w.walk, w.elapsed_ms, w.goal);
        for s in &w.steps {
            print_step(s);
        }
        println!("  = {}", w.path);
    }

    println!();
    let waited = r
        .potentialities
        .iter()
        .filter(|p| p.resolution == Resolution::Waited)
        .count();
    println!(
        "potentialities: {} ({} waited)",
        r.potentialities.len(),
        waited
    );
    for p in &r.potentialities {
        let kind = match (p.kind, p.resolution) {
            (PotentialityKind::Node, Resolution::Waited) => "node, waited   ",
            (PotentialityKind::Node, _) => "node, coexisted",
            (PotentialityKind::Conceptual, _) => "conceptual     ",
            (PotentialityKind::Alternative, _) => "alternative    ",
        };
        let mode = match p.mode {
            Some(Mode::Read) => "read ",
            Some(Mode::Write) => "write",
            None => "     ",
        };
        let with = p.with.map_or("         ".into(), |w| format!("⟂ walk {w}"));
        let wait = if p.wait_ms > 0.0 {
            format!("  {:.0}ms", p.wait_ms)
        } else {
            String::new()
        };
        println!(
            "  {kind} {mode}  walk {} {with} at {}  [{}]{wait}",
            p.walk,
            p.at,
            p.nodes.join(", ")
        );
    }

    println!();
    println!(
        "timing: wall {:.0}ms · model time {:.0}ms · parallelism {:.2}x",
        r.wall_ms,
        r.model_ms_sum,
        r.model_ms_sum / r.wall_ms.max(1e-9)
    );
    println!(
        "calls: judge {} ({} questions) · proposer {} ({} speculative discarded) · forks {} · tokens {} in / {} out",
        r.judge_calls,
        r.judge_questions,
        r.proposer_calls,
        r.speculative_discarded,
        r.forks,
        r.tokens_in,
        r.tokens_out
    );
    println!(
        "memory: rss {} → {} (peak {}) · heap {} → {} (peak {})",
        mib(r.mem_start.rss_bytes),
        mib(r.mem_end.rss_bytes),
        mib(r.peak_rss_bytes),
        mib(r.mem_start.heap_bytes),
        mib(r.mem_end.heap_bytes),
        mib(r.mem_end.heap_peak_bytes),
    );
    if let Some(path) = telemetry {
        println!("telemetry: {}", path.display());
    }
}

fn print_step(s: &StepRecord) {
    match s {
        StepRecord::Followed {
            from,
            arrow,
            to,
            decided_by,
            p,
            confidence,
            alternatives,
            attested,
            learned,
            wait_ms,
        } => {
            let learned = if *learned { "  (learned)" } else { "" };
            let conf = confidence.map_or(String::new(), |c| format!(" conf={c:.2}"));
            let wait = if *wait_ms >= 1.0 {
                format!("  waited {wait_ms:.0}ms")
            } else {
                String::new()
            };
            println!(
                "  {from} --{arrow}--> {to}   [{}] p={p:.2}{conf}{wait}{learned}",
                decided_by.as_str()
            );
            for a in attested {
                println!("      attested by {a}");
            }
            for a in alternatives {
                println!(
                    "      also held, not followed: {} -> {} (p={:.2})",
                    a.arrow, a.to, a.p
                );
            }
        }
        StepRecord::Forked {
            at,
            branches,
            fork_p,
            spawned,
            ..
        } => {
            let names: Vec<_> = branches
                .iter()
                .map(|b| format!("{} (p={:.2})", b.arrow, b.p))
                .collect();
            let ids: Vec<_> = spawned.iter().map(|w| format!("walk {w}")).collect();
            println!(
                "  {at} ⑂ fork p={fork_p:.2}: {} → spawned {}",
                names.join(", "),
                ids.join(", ")
            );
        }
        StepRecord::Escalated {
            at,
            reason,
            proposals,
            refused,
            held,
            ..
        } => {
            println!("  {at} ⇒ System 2 ({})", reason.as_str());
            for p in proposals {
                println!(
                    "    provisional {}: {} -> {}  — {}",
                    p.arrow, p.src, p.dst, p.about
                );
            }
            for (arrow, why) in refused {
                println!("    not learned {arrow}: {why}");
            }
            for (arrow, why) in held {
                println!("    held for a person {arrow}: {why}");
            }
        }
        StepRecord::Expanded {
            at,
            reason,
            source,
            learned,
            refused,
            held,
            ..
        } => {
            let by = if source == "proposer" {
                "System 2"
            } else {
                source.as_str()
            };
            println!(
                "  {at} ⇒ {by} ({}) ⇒ learned, walk continues",
                reason.as_str()
            );
            for p in learned {
                println!(
                    "    learned {}: {} -> {}  — {}",
                    p.arrow, p.src, p.dst, p.about
                );
            }
            for (arrow, why) in refused {
                println!("    refused {arrow}: {why}");
            }
            for (arrow, why) in held {
                println!("    held for a person {arrow}: {why}");
            }
        }
        StepRecord::Failed { at, error } => println!("  {at} ✗ {error}"),
        StepRecord::Joined {
            at,
            policy,
            role,
            detail,
            wait_ms,
            ..
        } => println!("  {at} ⤝ join ({policy}) {role}: {detail}   waited {wait_ms:.0}ms"),
    }
}

/// Each case's image under a functor: what the target category sees of
/// it. A case's walks (the root and its branches) are shown by the one
/// that got furthest in the target. A walk that leaves the functor's
/// domain (a learned arrow, an unmapped object) is shown up to there.
fn print_view(spec: &std::path::Path, r: &RunReport) -> Result<(), BoxError> {
    let (file, name) = crate::module::target(spec);
    let (module, _) = crate::module::load_module(&file)?;
    let f = match &name {
        Some(n) => module.functor(n),
        None if module.functors.len() == 1 => module.functors.first(),
        None => None,
    }
    .ok_or_else(|| format!("{}: name one functor with #Name", file.display()))?;
    let a = module.category(&f.src).expect("loaded");
    let b = module.category(&f.dst).expect("loaded");
    let parent: std::collections::HashMap<u64, Option<u64>> =
        r.walks.iter().map(|w| (w.walk, w.parent)).collect();
    let root = |mut w: u64| {
        while let Some(Some(p)) = parent.get(&w) {
            w = *p;
        }
        w
    };
    // Per case: (image length, image, where it left the view, goal).
    let mut cases: std::collections::BTreeMap<u64, (usize, String, Option<String>, String)> =
        Default::default();
    for w in &r.walks {
        // `h.g.f : A -> B`: arrows in reverse application order.
        let (arrows, ends) = w.path.split_once(" : ").unwrap_or(("", &w.path));
        let start = ends.split(" -> ").next().unwrap_or_default();
        let Ok(x) = a.object_id(start) else { continue };
        let mut path = onto_core::Path::id(x);
        let mut left = None;
        if !arrows.starts_with("id(") {
            for arrow in arrows.rsplit('.') {
                match a.arrow_id(arrow) {
                    Ok(id) if path.push(a, id).is_ok() => {}
                    _ => {
                        left = Some(arrow.to_owned());
                        break;
                    }
                }
            }
        }
        let Some(img) = f.image(a, b, &path) else {
            continue;
        };
        let entry = cases.entry(root(w.walk)).or_default();
        if img.arrows.len() >= entry.0 {
            *entry = (img.arrows.len(), img.display_typed(b), left, w.goal.clone());
        }
    }
    println!();
    println!(
        "view {}: {} -> {}   (each case, as far as it got)",
        f.name, f.src, f.dst
    );
    for (walk, (_, shown, left, goal)) in cases {
        let tail = left.map_or(String::new(), |l| {
            format!("   · left the view at `{l}` (not in {})", f.src)
        });
        let goal: String = goal.chars().take(48).collect();
        println!("  walk {walk:<3} {shown}{tail}");
        println!("           {goal}");
    }
    Ok(())
}

fn mib(bytes: Option<usize>) -> String {
    bytes.map_or("n/a".into(), |b| {
        format!("{:.2}MiB", b as f64 / (1024.0 * 1024.0))
    })
}
