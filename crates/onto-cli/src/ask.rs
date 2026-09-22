//! `onto ask`: one situation, answered from the beneficiary's side.
//!
//! The operator view (`onto run`) shows walks, claims and potentialities.
//! This view shows what the person who raised the case would be told:
//! handled, or needs a person and why. When the case has independent parts
//! the walk forks, and each part is reported on its own. AI suggestions are
//! marked as not active.

use std::io::{BufRead, IsTerminal, Write};
use std::path::PathBuf;
use std::sync::Arc;

use clap::Args;
use onto_core::walk::Escalation;
use onto_core::{Category, Closure};
use onto_runtime::engine::{StepRecord, WalkReport};
use onto_runtime::frames::PotentialityKind;
use onto_runtime::{Config, Engine, Job, Policy, RunReport, telemetry};

use crate::run::{BoxError, case, models};

#[derive(Args)]
pub struct AskArgs {
    file: PathBuf,
    /// Where the case enters the graph (e.g. `Ticket`).
    #[arg(long)]
    from: String,
    /// The situation in your own words, or a JSON object with a "goal" and
    /// structured facts. Omit to type several, one per line.
    text: Option<String>,
    #[arg(long)]
    mock: bool,
    #[arg(long)]
    mock_proposer: bool,
    #[arg(long, default_value_t = 0.6)]
    threshold: f32,
    #[arg(long)]
    telemetry: Option<PathBuf>,
}

pub fn main(args: AskArgs) -> Result<(), BoxError> {
    let src =
        std::fs::read_to_string(&args.file).map_err(|e| format!("{}: {e}", args.file.display()))?;
    let cat = Arc::new(onto_core::parse(&src)?);
    cat.object_id(&args.from)?;
    if let Some(path) = &args.telemetry {
        telemetry::init_jsonl(path)?;
    }
    let (judge, proposer) = models(args.mock, args.mock_proposer, None, None)?;
    let cfg = Config {
        threshold: args.threshold,
        policy: Policy::Shared,
        ..Config::default()
    };
    let engine = Engine::new(cat.clone(), judge, proposer, cfg);
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    let ask = |text: &str| -> Result<(), BoxError> {
        let (goal, case) = case(text)?;
        let job = Job {
            from: args.from.clone(),
            goal,
            case,
        };
        let report = rt.block_on(engine.run(vec![job]))?;
        print_answer(&cat, &report);
        Ok(())
    };

    if let Some(text) = &args.text {
        return ask(text);
    }
    let stdin = std::io::stdin();
    let interactive = stdin.is_terminal();
    if interactive {
        println!("Describe your situation (empty line or Ctrl-D to stop).");
    }
    loop {
        if interactive {
            print!("\n> ");
            std::io::stdout().flush()?;
        }
        let mut line = String::new();
        if stdin.lock().read_line(&mut line)? == 0 || line.trim().is_empty() {
            return Ok(());
        }
        ask(line.trim())?;
    }
}

fn print_answer(cat: &Category, r: &RunReport) {
    let root = &r.walks[0];
    println!();
    println!("  You: \"{}\"", root.goal);
    let parts: Vec<&WalkReport> = r.walks.iter().filter(|w| w.parent.is_some()).collect();
    if parts.is_empty() {
        print_walk(cat, r, root, "  ");
    } else {
        println!();
        println!(
            "  Your case has {} independent parts; each is handled on its own.",
            parts.len() + 1
        );
        for (i, w) in std::iter::once(root).chain(parts).enumerate() {
            println!();
            println!("  Part {}:", i + 1);
            print_walk(cat, r, w, "    ");
        }
    }
}

fn print_walk(cat: &Category, r: &RunReport, w: &WalkReport, pad: &str) {
    println!();
    let mut n = 0;
    let mut outcome = None;
    for s in &w.steps {
        match s {
            StepRecord::Followed {
                from,
                to,
                confidence,
                p,
                alternatives,
                ..
            } => {
                n += 1;
                let sure = confidence.unwrap_or(*p) * 100.0;
                println!("{pad}{n}. {from} → {to}   ({sure:.0}% sure)");
                for a in alternatives {
                    println!("{pad}   (also possible: {}, not pursued)", a.to);
                }
            }
            StepRecord::Forked { .. } => {}
            StepRecord::Escalated {
                at,
                reason,
                proposals,
                ..
            } => outcome = Some((at.clone(), *reason, proposals.clone())),
            StepRecord::Failed { at, error } => {
                println!("{pad}✗ Something went wrong at {at}: {error}");
                return;
            }
        }
    }
    if n == 0 {
        println!("{pad}(no existing option matched, even at the first step)");
    }

    let end_name = w.path.rsplit(" -> ").next().unwrap_or_default();
    let end = cat.object_id(end_name).ok();
    let terminal =
        end.is_some_and(|o| cat.object(o).closure == Closure::Closed && cat.out(o).is_empty());
    println!();
    match outcome {
        None if terminal => {
            let route = w.path.split(" : ").next().unwrap_or_default();
            println!("{pad}✓ Handled. This follows an existing route and reached {end_name}.");
            println!("{pad}  Route: {route}");
        }
        None => println!("{pad}… Stopped before reaching an outcome (step limit)."),
        Some((at, reason, proposals)) => {
            let why = match reason {
                Escalation::NoneOfThese => format!("none of the known options at {at} fits"),
                Escalation::LowConfidence => {
                    format!("at {at}, it was not sure enough which option fits")
                }
                Escalation::OpenFrame => format!("{at} is an area known to be incomplete"),
            };
            println!("{pad}⚠ Needs a person. The system stopped because {why},");
            println!(
                "{pad}  so it will not guess. This is recorded as a case it cannot handle yet."
            );
            if !proposals.is_empty() {
                println!();
                println!("{pad}Behind the scenes (NOT active, does not affect your case):");
                for p in &proposals {
                    println!(
                        "{pad}  an AI suggested a new option \"{}\" leading to {}",
                        p.arrow, p.dst
                    );
                    println!("{pad}    for: {}", p.about);
                }
                println!(
                    "{pad}  Suggestions stay provisional until someone validates them (coming in M2)."
                );
            }
        }
    }

    for p in r
        .potentialities
        .iter()
        .filter(|p| p.kind == PotentialityKind::Conceptual && p.walk == w.walk)
    {
        println!();
        println!(
            "{pad}↔ Someone earlier in this session raised a similar case (\"{}\"); the two would be reviewed together.",
            p.nodes.join(", ")
        );
    }
}
