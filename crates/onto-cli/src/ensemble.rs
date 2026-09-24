//! `onto ensemble FILE#Name --jobs JOBS`: walk each case in every column
//! of an ensemble and compare the columns in the shared category.
//!
//! Each column is its own engine, with its own learned layer, `state`
//! policy and judge; the columns never see each other. Jobs are one case
//! per line (`{json}` or `Label: {json}`); each column starts where the
//! ensemble declares.

use std::path::PathBuf;
use std::sync::Arc;

use clap::Args;
use onto_runtime::ensemble::{ColumnRun, run};
use onto_runtime::{Config, Engine, Job, telemetry};

use crate::run::{BoxError, case, models};

#[derive(Args)]
pub struct EnsembleArgs {
    /// `FILE#Ensemble` (or `FILE` when it declares one ensemble).
    file: PathBuf,
    /// One case per line.
    #[arg(long)]
    jobs: PathBuf,
    #[arg(long)]
    mock: bool,
    #[arg(long)]
    mock_proposer: bool,
    #[arg(long, default_value_t = 0.6)]
    threshold: f32,
    #[arg(long, default_value_t = 4)]
    max_branches: usize,
    #[arg(long)]
    telemetry: Option<PathBuf>,
    /// Frame records of every column (JSON lines).
    #[arg(long)]
    dispositions: Option<PathBuf>,
    /// The full ensemble report as JSON.
    #[arg(long)]
    report: Option<PathBuf>,
    #[command(flatten)]
    world: crate::learned::WorldArgs,
}

pub fn main(args: EnsembleArgs) -> Result<(), BoxError> {
    let (file, name) = crate::module::target(&args.file);
    let (module, _) = crate::module::load_module(&file)?;
    let e = match &name {
        Some(n) => module.ensemble(n),
        None if module.ensembles.len() == 1 => module.ensembles.first(),
        None => None,
    }
    .ok_or_else(|| format!("{}: name one ensemble with #Name", file.display()))?
    .clone();
    let shared = Arc::new(module.category(&e.shared).expect("validated").clone());
    if let Some(path) = &args.telemetry {
        telemetry::init_jsonl(path)?;
    }
    let jobs = read_cases(&args.jobs)?;
    println!(
        "ensemble {}: {} columns → {} · consensus {} · {} case(s)",
        e.name,
        e.columns.len(),
        e.shared,
        match e.consensus {
            onto_core::ensemble::Consensus::All => "all".to_owned(),
            onto_core::ensemble::Consensus::Quorum(q) => format!("quorum {q}"),
        },
        jobs.len()
    );
    let mut columns = Vec::new();
    for (k, c) in e.columns.iter().enumerate() {
        let spec = PathBuf::from(format!("{}#{}", file.display(), c.category));
        let cat = args.world.part(&c.category).load(&spec)?;
        let (judge, proposer) = models(args.mock, args.mock_proposer, None, None)?;
        let cfg = Config {
            threshold: args.threshold,
            max_branches: args.max_branches,
            open_world: !args.world.closed_world,
            max_expansions: args.world.max_expansions,
            assured: args.world.assured(),
            ensemble: Some(e.name.clone()),
            ..Config::default()
        };
        let engine = Engine::new(cat, judge, proposer, cfg);
        engine.with_walk_base(1 + k as u64 * 1_000_000);
        columns.push(ColumnRun {
            name: c.category.clone(),
            engine,
            functor: module.functor(&c.functor).expect("validated").clone(),
            start: c.start.clone(),
        });
    }
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let report = rt.block_on(run(&e, shared, columns, jobs))?;

    for c in &report.cases {
        println!();
        let label = c
            .case
            .clone()
            .unwrap_or_else(|| format!("case {}", c.job + 1));
        let goal: String = c.goal.chars().take(70).collect();
        println!("  {label}: {goal}");
        for p in &c.positions {
            println!(
                "    {:<14} at {:<22} → {}",
                p.column,
                p.object,
                p.shared.clone().unwrap_or_default()
            );
        }
        let when = |x: Option<f64>| x.map_or(String::new(), |ms| format!(" (first at {ms:.0} ms)"));
        match c.status.as_str() {
            "agreed" => {
                println!(
                    "    ✓ agreed: {}{}{}",
                    c.agreed.clone().unwrap_or_default(),
                    when(c.first_confirm_ms),
                    if c.dissent.is_empty() {
                        String::new()
                    } else {
                        format!(" · dissent: {}", c.dissent.join(", "))
                    }
                );
            }
            "surprise" => {
                let pairs: Vec<String> = c
                    .contradictions
                    .iter()
                    .map(|(a, b)| format!("{a} ⟂ {b}"))
                    .collect();
                println!(
                    "    ⚠ surprise: {}{} → {}",
                    pairs.join(", "),
                    when(c.first_surprise_ms),
                    match c.route.as_deref() {
                        Some("person") => "needs a person (the perspectives disagree)",
                        _ => "a curation signal (the model of the world may be wrong here)",
                    }
                );
            }
            "undecided" => println!("    … undecided: no column concluded anything"),
            _ => println!("    … incomplete: a column never reached {}", report.shared),
        }
    }
    println!();
    println!("wall {:.0} ms", report.wall_ms);
    if let Some(path) = &args.dispositions {
        let mut out = String::new();
        for r in &report.runs {
            for w in &r.walks {
                for f in &w.frames {
                    out.push_str(&serde_json::to_string(f)?);
                    out.push('\n');
                }
            }
        }
        std::fs::write(path, out)?;
    }
    if let Some(path) = &args.report {
        std::fs::write(path, serde_json::to_string_pretty(&report)?)?;
    }
    for (c, r) in e.columns.iter().zip(&report.runs) {
        let spec = PathBuf::from(format!("{}#{}", file.display(), c.category));
        let world = args.world.part(&c.category);
        world.save(&spec, &r.learned)?;
        world.save_gaps(&spec, &r.gaps)?;
    }
    // Contradictions are gaps of the shared category.
    args.world.part(&e.shared).save_gaps(
        &PathBuf::from(format!("{}#{}", file.display(), e.shared)),
        &report.gaps,
    )?;
    Ok(())
}

/// One case per line: `{json}` or `Label: {json}`.
fn read_cases(path: &std::path::Path) -> Result<Vec<Job>, BoxError> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut jobs = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let json = line.find('{').map_or(line, |at| &line[at..]);
        let (goal, case) = case(json).map_err(|e| format!("{}:{}: {e}", path.display(), i + 1))?;
        jobs.push(Job {
            from: String::new(),
            goal,
            case,
        });
    }
    Ok(jobs)
}
