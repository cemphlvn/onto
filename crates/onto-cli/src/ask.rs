//! `onto ask`: one situation, answered from the beneficiary's side.
//!
//! The operator view (`onto run`) shows walks, claims and potentialities.
//! This view shows what the person who raised the case would be told:
//! handled, or needs a person and why, with any AI suggestion clearly
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

use crate::run::{BoxError, models};

#[derive(Args)]
pub struct AskArgs {
    file: PathBuf,
    /// Where the case enters the graph (e.g. `Ticket`).
    #[arg(long)]
    from: String,
    /// The situation in your own words. Omit to type several, one per line.
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
    let (chooser, proposer) = models(args.mock, args.mock_proposer, None, None)?;
    let cfg = Config {
        threshold: args.threshold,
        policy: Policy::Shared,
        ..Config::default()
    };
    let engine = Engine::new(cat.clone(), chooser, proposer, cfg);
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    let ask = |text: String| -> Result<(), BoxError> {
        let job = Job {
            from: args.from.clone(),
            goal: text,
        };
        let report = rt.block_on(engine.run(vec![job]))?;
        print_answer(&cat, &report);
        Ok(())
    };

    if let Some(text) = args.text.clone() {
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
        ask(line.trim().to_owned())?;
    }
}

fn print_answer(cat: &Category, r: &RunReport) {
    let w: &WalkReport = &r.walks[0];
    println!();
    println!("  You: \"{}\"", w.goal);
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
                ..
            } => {
                n += 1;
                let sure = confidence.unwrap_or(*p) * 100.0;
                println!("  {n}. {from} → {to}   ({sure:.0}% sure)");
            }
            StepRecord::Escalated {
                at,
                reason,
                proposals,
                ..
            } => outcome = Some((at.clone(), Some(*reason), proposals.clone())),
            StepRecord::Failed { at, error } => {
                println!("  ✗ Something went wrong at {at}: {error}");
                return;
            }
        }
    }

    let end = cat
        .object_id(w.path.split(" -> ").last().unwrap_or_default())
        .ok();
    let terminal =
        end.is_some_and(|o| cat.object(o).closure == Closure::Closed && cat.out(o).is_empty());
    println!();
    match outcome {
        None if terminal => {
            let route = w.path.split(" : ").next().unwrap_or_default();
            println!(
                "  ✓ Handled. Your case follows an existing route and reached {}.",
                cat.object(end.unwrap()).name
            );
            println!("    Route: {route}");
        }
        None => println!("  … Stopped before reaching an outcome (step limit)."),
        Some((at, reason, proposals)) => {
            let why = match reason {
                Some(Escalation::NoneOfThese) => {
                    format!("none of the known options at {at} fits your case")
                }
                Some(Escalation::LowConfidence) => {
                    format!("at {at}, it was not sure enough which option fits")
                }
                Some(Escalation::OpenFrame) => format!("{at} is an area known to be incomplete"),
                None => String::new(),
            };
            println!("  ⚠ Needs a person. The system stopped because {why},");
            println!(
                "    so it will not guess. Your case is recorded as one it cannot handle yet."
            );
            if !proposals.is_empty() {
                println!();
                println!("  Behind the scenes (NOT active, does not affect your case):");
                for p in &proposals {
                    println!(
                        "    an AI suggested a new option \"{}\" leading to {}",
                        p.arrow, p.dst
                    );
                    println!("      because: {}", p.rationale);
                }
                println!(
                    "    Suggestions stay provisional until someone validates them (coming in M2)."
                );
            }
        }
    }

    let earlier: Vec<_> = r
        .potentialities
        .iter()
        .filter(|p| p.kind == PotentialityKind::Conceptual && p.walk == w.walk)
        .collect();
    for p in earlier {
        println!();
        println!(
            "  ↔ Someone earlier in this session raised a similar case (\"{}\"); the two would be reviewed together.",
            p.nodes.join(", ")
        );
    }
}
