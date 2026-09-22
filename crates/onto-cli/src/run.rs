//! `onto run`: the concurrent core loop from the command line.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use clap::{Args, ValueEnum};
use onto_core::walk::{Distribution, Proposal};
use onto_runtime::engine::StepRecord;
use onto_runtime::frames::{Mode, PotentialityKind, Resolution};
use onto_runtime::model::{
    ChoiceRequest, Chooser, MockChooser, MockProposer, ModelError, ProposalRequest, Proposer, Usage,
};
use onto_runtime::providers::{Jev, OpenRouter};
use onto_runtime::{Config, Engine, Job, Policy, RunReport, telemetry};

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
    #[arg(long, value_enum, default_value_t = PolicyArg::Exclusive)]
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
    /// Write the full run report as JSON to this file.
    #[arg(long)]
    report: Option<PathBuf>,
    #[arg(long)]
    chooser_model: Option<String>,
    #[arg(long)]
    proposer_model: Option<String>,
}

#[derive(Clone, Copy, ValueEnum)]
enum PolicyArg {
    Exclusive,
    Shared,
}

enum AnyChooser {
    Jev(Jev),
    Mock(MockChooser),
}

impl Chooser for AnyChooser {
    fn name(&self) -> String {
        match self {
            Self::Jev(c) => c.name(),
            Self::Mock(c) => c.name(),
        }
    }
    async fn choose(&self, req: ChoiceRequest) -> Result<(Distribution, Usage), ModelError> {
        match self {
            Self::Jev(c) => c.choose(req).await,
            Self::Mock(c) => c.choose(req).await,
        }
    }
}

enum AnyProposer {
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

type BoxError = Box<dyn std::error::Error>;

pub fn main(args: RunArgs) -> Result<(), BoxError> {
    let src =
        std::fs::read_to_string(&args.file).map_err(|e| format!("{}: {e}", args.file.display()))?;
    let cat = Arc::new(onto_core::parse(&src)?);
    let jobs = read_jobs(&args.jobs)?;
    if let Some(path) = &args.telemetry {
        telemetry::init_jsonl(path)?;
    }

    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()?;
    let chooser = if args.mock {
        AnyChooser::Mock(MockChooser {
            latency: Duration::from_millis(150),
        })
    } else {
        AnyChooser::Jev(
            Jev::from_env(http.clone(), args.chooser_model.clone())
                .ok_or("TYPESAFE_API_KEY is not set (or pass --mock)")?,
        )
    };
    let proposer = if args.mock || args.mock_proposer {
        AnyProposer::Mock(MockProposer {
            latency: Duration::from_millis(600),
        })
    } else {
        AnyProposer::OpenRouter(
            OpenRouter::from_env(http, args.proposer_model.clone())
                .ok_or("OPENROUTER_API_KEY is not set (or pass --mock-proposer)")?,
        )
    };
    let cfg = Config {
        threshold: args.threshold,
        policy: match args.policy {
            PolicyArg::Exclusive => Policy::Exclusive,
            PolicyArg::Shared => Policy::Shared,
        },
        speculate: args.speculate,
        max_steps: args.max_steps,
        ..Config::default()
    };

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let report = rt.block_on(Engine::new(cat, chooser, proposer, cfg).run(jobs))?;

    print_summary(&report, args.telemetry.as_deref());
    if let Some(path) = &args.report {
        std::fs::write(path, serde_json::to_string_pretty(&report)?)?;
        println!("report: {}", path.display());
    }
    Ok(())
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
        jobs.push(Job {
            from: from.trim().to_owned(),
            goal: goal.trim().to_owned(),
        });
    }
    Ok(jobs)
}

fn print_summary(r: &RunReport, telemetry: Option<&std::path::Path>) {
    println!(
        "run: {} walks · chooser {} · proposer {} · policy {:?} · speculate {}",
        r.walks.len(),
        r.chooser,
        r.proposer,
        r.policy,
        if r.speculate { "on" } else { "off" }
    );
    println!();
    for w in &r.walks {
        println!("walk {} [{:.0}ms] {}", w.walk, w.elapsed_ms, w.goal);
        for s in &w.steps {
            match s {
                StepRecord::Followed {
                    from,
                    arrow,
                    to,
                    p,
                    confidence,
                    wait_ms,
                } => {
                    let conf = confidence.map_or(String::new(), |c| format!(" conf={c:.2}"));
                    let wait = if *wait_ms >= 1.0 {
                        format!("  waited {wait_ms:.0}ms")
                    } else {
                        String::new()
                    };
                    println!("  {from} --{arrow}--> {to}   p={p:.2}{conf}{wait}");
                }
                StepRecord::Escalated {
                    at,
                    reason,
                    proposals,
                    ..
                } => {
                    println!("  {at} ⇒ System 2 ({reason:?})");
                    for p in proposals {
                        println!(
                            "    provisional {}: {} -> {}  — {}",
                            p.arrow, p.src, p.dst, p.rationale
                        );
                    }
                }
                StepRecord::Failed { at, error } => println!("  {at} ✗ {error}"),
            }
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
            (PotentialityKind::Node, Resolution::Coexisted) => "node, coexisted",
            (PotentialityKind::Conceptual, _) => "conceptual     ",
        };
        let mode = match p.mode {
            Some(Mode::Read) => "read ",
            Some(Mode::Write) => "write",
            None => "     ",
        };
        let wait = if p.wait_ms > 0.0 {
            format!("  {:.0}ms", p.wait_ms)
        } else {
            String::new()
        };
        println!(
            "  {kind} {mode}  walk {} ⟂ walk {} at {}  [{}]{wait}",
            p.walk,
            p.with,
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
        "calls: chooser {} · proposer {} ({} speculative discarded) · tokens {} in / {} out",
        r.chooser_calls, r.proposer_calls, r.speculative_discarded, r.tokens_in, r.tokens_out
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

fn mib(bytes: Option<usize>) -> String {
    bytes.map_or("n/a".into(), |b| {
        format!("{:.2}MiB", b as f64 / (1024.0 * 1024.0))
    })
}
