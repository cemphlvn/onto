//! `onto ask`: one situation, answered from the beneficiary's side.
//!
//! The operator view (`onto run`) shows walks, claims and potentialities.
//! This view shows what the person who raised the case would be told:
//! handled, or needs a person and why. When the case has independent parts
//! the walk forks, and each part is reported on its own. AI suggestions are
//! marked as not active.

use std::io::{BufRead, IsTerminal, Write};
use std::path::PathBuf;

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
    /// Also show why: every option considered at each step and what
    /// happened to it.
    #[arg(long)]
    why: bool,
    #[command(flatten)]
    world: crate::learned::WorldArgs,
}

pub fn main(args: AskArgs) -> Result<(), BoxError> {
    let cat = args.world.load(&args.file)?;
    cat.object_id(&args.from)?;
    if let Some(path) = &args.telemetry {
        telemetry::init_jsonl(path)?;
    }
    let (judge, proposer) = models(args.mock, args.mock_proposer, None, None)?;
    let cfg = Config {
        threshold: args.threshold,
        policy: Policy::Shared,
        open_world: !args.world.closed_world,
        max_expansions: args.world.max_expansions,
        assured: args.world.assured(),
        // An interactive session shows what an AI would suggest.
        review_inline: true,
        ..Config::default()
    };
    let precedents = args.world.precedents(&args.file, &cat)?;
    let (lenses, entries) = args.world.lenses(&args.file, &cat)?;
    let engine = Engine::new(cat, judge, proposer, cfg);
    engine.remember(precedents);
    engine.use_lenses(lenses, &entries);
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
        // The graph may have grown during the walk (open world).
        print_answer(&engine.cat(), &report);
        args.world.save(&args.file, &report.learned)?;
        args.world.save_precedents(&args.file, &report.precedents)?;
        if args.why {
            println!();
            println!("  Why:");
            for w in &report.walks {
                for f in &w.frames {
                    println!();
                    crate::why::print_record(&serde_json::to_value(f)?, "    ");
                }
            }
        }
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
    let mut combined = false;
    let mut last_attested: Vec<String> = Vec::new();
    // Set when a join this walk continued from was attested completion.
    let mut attested_join: Option<String> = None;
    // Some step went through structure the open world learned.
    let mut through_learned = false;
    for s in &w.steps {
        match s {
            StepRecord::Followed {
                from,
                to,
                decided_by,
                confidence,
                p,
                alternatives,
                attested,
                learned,
                ..
            } => {
                n += 1;
                through_learned |= *learned;
                if *decided_by == onto_core::Primitive::Split {
                    println!("{pad}{n}. {from} → {to}   (a required step: no judgment)");
                } else {
                    let sure = confidence.unwrap_or(*p) * 100.0;
                    let learned = if *learned {
                        "; a newly learned option"
                    } else {
                        ""
                    };
                    println!("{pad}{n}. {from} → {to}   ({sure:.0}% sure{learned})");
                }
                for a in attested {
                    println!("{pad}   attested by {a}");
                }
                last_attested = attested.clone();
                for a in alternatives {
                    println!("{pad}   (also possible: {}, not pursued)", a.to);
                }
            }
            StepRecord::Forked { .. } => {}
            StepRecord::Joined {
                at,
                role,
                into,
                detail,
                completion_attested,
                ..
            } => match role.as_str() {
                "continued" if *completion_attested => {
                    println!(
                        "{pad}   ⤝ the parts of your case came back together at {at} ({detail})"
                    );
                    attested_join = Some(if detail.starts_with("authorized") {
                        format!("authorized at {at} on attested evidence")
                    } else {
                        format!("every check at {at} was attested")
                    });
                }
                "continued" => println!(
                    "{pad}   ⤝ the parts of your case came back together at {at} ({detail})"
                ),
                "ended" => {
                    let target = into.map_or(String::new(), |w| format!(" (walk {w})"));
                    println!("{pad}   ⤝ this part was combined into another part{target} at {at}");
                    combined = true;
                }
                _ => {}
            },
            StepRecord::Escalated {
                at,
                reason,
                proposals,
                ..
            } => outcome = Some((at.clone(), *reason, proposals.clone())),
            StepRecord::Expanded {
                at,
                learned,
                source,
                ..
            } => {
                let from = match source.strip_prefix("transport ") {
                    Some(f) => {
                        format!("a related catalogue ({f}) already knew options this one lacked")
                    }
                    None => "an AI proposed new ones".into(),
                };
                println!(
                    "{pad}   ⟡ the known options at {at} did not cover your case; {from}, they passed the safety proofs (policy, capabilities, invariants), and the walk continued:"
                );
                for p in learned {
                    println!("{pad}     learned \"{}\" → {}: {}", p.arrow, p.dst, p.about);
                }
            }
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
            // Completion in the world is claimed only on attested evidence.
            if !last_attested.is_empty() {
                println!(
                    "{pad}✓ Completed: {end_name}, attested by {}.",
                    last_attested.join("; ")
                );
            } else if let Some(how) = &attested_join {
                println!("{pad}✓ Completed: {end_name}; {how}.");
            } else if through_learned {
                println!(
                    "{pad}✓ Reached {end_name} through learned structure (not declared policy)."
                );
                println!(
                    "{pad}  Not attested: no trusted source has confirmed the outcome in the world."
                );
            } else {
                println!("{pad}✓ Reached {end_name} by an existing route.");
                println!(
                    "{pad}  Not attested: no trusted source has confirmed the outcome in the world."
                );
            }
            println!("{pad}  Route: {route}");
        }
        None if combined => println!(
            "{pad}✓ Combined into another part of your case; see that part for the outcome."
        ),
        None => println!("{pad}… Stopped before reaching an outcome (step limit)."),
        Some((at, reason, proposals)) => {
            let why = match reason {
                Escalation::NoneOfThese => format!("none of the known options at {at} fits"),
                Escalation::LowConfidence => {
                    format!("at {at}, it was not sure enough which option fits")
                }
                Escalation::OpenFrame => format!("{at} is an area known to be incomplete"),
                Escalation::IncompleteJoin => {
                    format!(
                        "the parts of your case were meant to come together at {at}, but one part could not finish"
                    )
                }
                Escalation::SplitOverBudget => {
                    format!("{at} has required steps that could not all be started")
                }
                Escalation::BlockedByGate => {
                    format!(
                        "{at} needed an authorization from another part of your case that never arrived"
                    )
                }
            };
            // Every option here waits on attested evidence: say so plainly.
            let waiting = w.frames.last().is_some_and(|f| {
                !f.candidates.is_empty()
                    && f.candidates
                        .iter()
                        .all(|c| c.disposition == onto_core::walk::Disposition::Unattested)
            });
            let why = if waiting {
                format!(
                    "at {at} it is waiting for evidence that no trusted source has provided yet"
                )
            } else {
                why
            };
            println!("{pad}⚠ Needs a person. The system stopped because {why},");
            println!(
                "{pad}  so it will not guess. This is recorded as a case it cannot handle yet."
            );
            if let Some(last) = w.frames.last() {
                for c in last
                    .candidates
                    .iter()
                    .filter(|c| c.disposition == onto_core::walk::Disposition::FilteredByRequire)
                {
                    println!(
                        "{pad}  {} is not allowed for this case: it requires `{}`, which your case does not show.",
                        c.to,
                        c.require.as_deref().unwrap_or("?")
                    );
                }
                for c in last
                    .candidates
                    .iter()
                    .filter(|c| c.disposition == onto_core::walk::Disposition::BlockedByEntry)
                {
                    println!("{pad}  {} is not open to this case: {}.", c.to, c.reason);
                }
                for c in last
                    .candidates
                    .iter()
                    .filter(|c| c.disposition == onto_core::walk::Disposition::Unattested)
                {
                    println!(
                        "{pad}  {} is waiting for evidence from a trusted source: {}.",
                        c.to, c.reason
                    );
                }
            }
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
                    "{pad}  Suggestions stay provisional until a person reviews and promotes them."
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
