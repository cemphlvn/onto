use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use onto_core::walk::{Escalation, Judge, NullProposer, ScriptedJudge, Step, UniformJudge, Walker};
use onto_core::{Category, Closure, Equality, Verdict, category::resolve, parse::path_spec};

mod ask;
mod attest;
mod curate;
mod ensemble;
mod functor;
mod laws;
mod learned;
mod migrate;
mod module;
mod quotient;
mod raster;
mod replay;
mod run;
mod supervise;
mod why;

#[global_allocator]
static ALLOC: onto_runtime::mem::CountingAlloc = onto_runtime::mem::CountingAlloc;

/// Navigate categories defined in `.onto` files.
#[derive(Parser)]
#[command(name = "onto", version)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Validate a file and summarise the category.
    Check {
        file: PathBuf,
    },
    /// List an object's decision frame (its outgoing arrows).
    Ls {
        file: PathBuf,
        object: String,
    },
    /// Type-check a path such as `g.f` and show the simplest equal path.
    Compose {
        file: PathBuf,
        path: String,
    },
    /// Decide whether two paths are equal.
    Eq {
        file: PathBuf,
        lhs: String,
        rhs: String,
    },
    /// List every route between two objects, grouped by path equality.
    Reach {
        file: PathBuf,
        from: String,
        to: String,
        /// Objects no route may pass through, comma separated.
        #[arg(long, value_delimiter = ',')]
        avoid: Vec<String>,
        #[arg(long, default_value_t = 8)]
        max_len: usize,
    },
    /// Walk from an object, System 1 first, escalating when the frame runs out.
    Walk {
        file: PathBuf,
        #[arg(long)]
        from: String,
        /// Scripted System-1 choices, comma separated (default: uniform).
        #[arg(long, value_delimiter = ',')]
        script: Option<Vec<String>>,
        #[arg(long, default_value_t = 16)]
        steps: usize,
        #[arg(long, default_value_t = 0.5)]
        threshold: f32,
        #[arg(long, default_value = "")]
        goal: String,
    },
    /// Generate an Ed25519 key pair for an attester.
    Keygen(attest::KeygenArgs),
    /// Sign an observation as an attester (prints the observation JSON).
    Attest(attest::AttestArgs),
    /// Behavioural equivalence classes: duplicates and description-only identities.
    Quotient(quotient::QuotientArgs),
    /// Derive what entry contracts and arrow effects imply.
    Laws(laws::LawsArgs),
    /// Review provisional proposals: structural proofs, then semantic checks.
    Review(supervise::ReviewArgs),
    /// Promote one reviewed proposal into the category file (the human step).
    Promote(supervise::PromoteArgs),
    /// Explain every decision in a disposition file (from `run --dispositions`).
    Why(why::WhyArgs),
    /// Try a case as the person raising it: one live walk, answered in plain terms.
    Ask(ask::AskArgs),
    /// Run many walks concurrently against live models (Jev + OpenRouter) or mocks.
    Run(run::RunArgs),
    /// List the learned layer (open world) and whether each arrow still
    /// holds against the declared graph.
    /// What a functor keeps, reflects and leaves uncovered.
    Functor(functor::FunctorArgs),
    /// Draw what a run did, in time: frames × time, from its telemetry.
    Raster(raster::RasterArgs),
    /// One proposal per gap signal group, for review (`onto review`).
    Curate(curate::CurateArgs),
    /// Walk each case in every column of an ensemble; compare the columns.
    Ensemble(ensemble::EnsembleArgs),
    Learned(learned::LearnedArgs),
    /// Carry the learned layer and memory along a version functor.
    Migrate(migrate::MigrateArgs),
    /// Ask the judge again with a recorded state changed (counterfactual):
    /// does the decision depend on that field?
    Replay(replay::ReplayArgs),
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.cmd {
        Cmd::Run(args) => run::main(args).map(|()| ExitCode::SUCCESS),
        Cmd::Functor(args) => functor::main(args).map(|()| ExitCode::SUCCESS),
        Cmd::Raster(args) => raster::main(args).map(|()| ExitCode::SUCCESS),
        Cmd::Curate(args) => curate::main(args).map(|()| ExitCode::SUCCESS),
        Cmd::Ensemble(args) => ensemble::main(args).map(|()| ExitCode::SUCCESS),
        Cmd::Learned(args) => learned::main(args).map(|()| ExitCode::SUCCESS),
        Cmd::Migrate(args) => migrate::main(args).map(|()| ExitCode::SUCCESS),
        Cmd::Replay(args) => replay::main(args).map(|()| ExitCode::SUCCESS),
        Cmd::Ask(args) => ask::main(args).map(|()| ExitCode::SUCCESS),
        Cmd::Why(args) => why::main(args).map(|()| ExitCode::SUCCESS),
        Cmd::Laws(args) => laws::main(args).map(|()| ExitCode::SUCCESS),
        Cmd::Quotient(args) => quotient::main(args).map(|()| ExitCode::SUCCESS),
        Cmd::Keygen(args) => attest::keygen(args).map(|()| ExitCode::SUCCESS),
        Cmd::Attest(args) => attest::attest(args).map(|()| ExitCode::SUCCESS),
        Cmd::Review(args) => supervise::review_main(args).map(|()| ExitCode::SUCCESS),
        Cmd::Promote(args) => supervise::promote_main(args).map(|()| ExitCode::SUCCESS),
        cmd => run(Cli { cmd }),
    };
    result.unwrap_or_else(|e| {
        eprintln!("error: {e}");
        ExitCode::FAILURE
    })
}

fn load(file: &std::path::Path) -> Result<Category, Box<dyn std::error::Error>> {
    module::load_category(file)
}

fn run(cli: Cli) -> Result<ExitCode, Box<dyn std::error::Error>> {
    match cli.cmd {
        Cmd::Check { file } => {
            let cat = load(&file)?;
            let closed = cat
                .objects()
                .iter()
                .filter(|o| o.closure == Closure::Closed)
                .count();
            println!(
                "category {}: {} objects ({} closed frames), {} arrows, {} equations",
                cat.name(),
                cat.objects().len(),
                closed,
                cat.arrows().len(),
                cat.equations().len()
            );
            for inv in cat.invariants() {
                println!("  invariant {inv}");
            }
            Equality::new(&cat)?;
            println!("ok");
        }
        Cmd::Ls { file, object } => {
            let cat = load(&file)?;
            let obj = cat.object_id(&object)?;
            let closure = match cat.object(obj).closure {
                Closure::Closed => "closed (MECE)",
                Closure::Open => "open",
            };
            println!("{object}  [{closure}]");
            for a in cat.out(obj) {
                let a = cat.arrow(*a);
                println!("  {} -> {}", a.name, cat.object(a.dst).name);
            }
        }
        Cmd::Compose { file, path } => {
            let cat = load(&file)?;
            let p = resolve(&cat, &path_spec(&path)?)?;
            println!("{}", p.display_typed(&cat));
            let simplest = Equality::new(&cat)?.simplest(&p);
            if simplest != p {
                println!("  = {}", simplest.display(&cat));
            }
        }
        Cmd::Eq { file, lhs, rhs } => {
            let cat = load(&file)?;
            let a = resolve(&cat, &path_spec(&lhs)?)?;
            let b = resolve(&cat, &path_spec(&rhs)?)?;
            let verdict = Equality::new(&cat)?.check(&a, &b);
            println!("{verdict:?}");
            if verdict != Verdict::Equal {
                return Ok(ExitCode::from(1));
            }
        }
        Cmd::Reach {
            file,
            from,
            to,
            avoid,
            max_len,
        } => {
            let cat = load(&file)?;
            let avoid = avoid
                .iter()
                .map(|n| cat.object_id(n))
                .collect::<Result<Vec<_>, _>>()?;
            let paths = cat.paths(cat.object_id(&from)?, cat.object_id(&to)?, &avoid, max_len);
            if paths.is_empty() {
                let via = if avoid.is_empty() {
                    String::new()
                } else {
                    let names: Vec<_> =
                        avoid.iter().map(|o| cat.object(*o).name.as_str()).collect();
                    format!(" avoiding {}", names.join(", "))
                };
                println!("no path {from} -> {to}{via} (max {max_len} arrows)");
                return Ok(ExitCode::from(1));
            }
            // Group routes the declared equations prove equal.
            let eq = Equality::new(&cat)?;
            let mut classes: Vec<Vec<&onto_core::Path>> = Vec::new();
            for p in &paths {
                match classes
                    .iter_mut()
                    .find(|c| eq.check(c[0], p) == Verdict::Equal)
                {
                    Some(c) => c.push(p),
                    None => classes.push(vec![p]),
                }
            }
            println!(
                "{} path(s) {from} -> {to}, {} distinct up to equations:",
                paths.len(),
                classes.len()
            );
            for c in classes {
                let routes: Vec<_> = c.iter().map(|p| p.display(&cat)).collect();
                println!("  {}", routes.join("  ≡  "));
            }
        }
        Cmd::Walk {
            file,
            from,
            script,
            steps,
            threshold,
            goal,
        } => {
            let cat = load(&file)?;
            let from = cat.object_id(&from)?;
            let judge: Box<dyn Judge> = match script {
                Some(names) => Box::new(ScriptedJudge::new(names)),
                None => Box::new(UniformJudge),
            };
            let mut walker = Walker {
                cat: &cat,
                judge,
                proposer: NullProposer,
                threshold,
            };
            let walk = walker.walk(&goal, serde_json::json!({}), from, steps);
            let mut at = from;
            for step in &walk.steps {
                match step {
                    Step::Followed { arrow, p } => {
                        let a = cat.arrow(*arrow);
                        println!(
                            "{} --{}--> {}   p={p:.2}",
                            cat.object(at).name,
                            a.name,
                            cat.object(a.dst).name
                        );
                        at = a.dst;
                    }
                    Step::Escalated { reason, proposals } => {
                        let why = match reason {
                            Escalation::OpenFrame => "frame is open (not MECE)",
                            Escalation::NoneOfThese => "System 1 found no option that fits",
                            Escalation::LowConfidence => "System 1 below threshold",
                            Escalation::IncompleteJoin => "join could not complete",
                            Escalation::BlockedByGate => "join gate never authorized",
                            Escalation::SplitOverBudget => "split could not pursue every arrow",
                        };
                        println!("{} ⇒ escalate to System 2: {why}", cat.object(at).name);
                        for p in proposals {
                            println!(
                                "  provisional {}: {} -> {}  ({})",
                                p.arrow, p.src, p.dst, p.rationale
                            );
                        }
                    }
                }
            }
            println!("path: {}", walk.state.path.display_typed(&cat));
        }
        Cmd::Run(_)
        | Cmd::Learned(_)
        | Cmd::Ensemble(_)
        | Cmd::Curate(_)
        | Cmd::Raster(_)
        | Cmd::Migrate(_)
        | Cmd::Functor(_)
        | Cmd::Replay(_)
        | Cmd::Ask(_)
        | Cmd::Why(_)
        | Cmd::Review(_)
        | Cmd::Promote(_)
        | Cmd::Laws(_)
        | Cmd::Quotient(_)
        | Cmd::Keygen(_)
        | Cmd::Attest(_) => {
            unreachable!("handled in main")
        }
    }
    Ok(ExitCode::SUCCESS)
}
