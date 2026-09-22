use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use onto_core::walk::{
    Chooser, Escalation, NullProposer, ScriptedChooser, Step, UniformChooser, Walker,
};
use onto_core::{Category, Closure, Equality, Verdict, category::resolve, parse, parse::path_spec};

mod run;

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
    Check { file: PathBuf },
    /// List an object's decision frame (its outgoing arrows).
    Ls { file: PathBuf, object: String },
    /// Type-check a path such as `g.f` and show the simplest equal path.
    Compose { file: PathBuf, path: String },
    /// Decide whether two paths are equal.
    Eq {
        file: PathBuf,
        lhs: String,
        rhs: String,
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
    /// Run many walks concurrently against live models (Jev + OpenRouter) or mocks.
    Run(run::RunArgs),
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    if let Cmd::Run(args) = cli.cmd {
        return match run::main(args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::FAILURE
            }
        };
    }
    match run(cli) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn load(file: &PathBuf) -> Result<Category, Box<dyn std::error::Error>> {
    let src = std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
    Ok(parse(&src)?)
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
            let chooser: Box<dyn Chooser> = match script {
                Some(names) => Box::new(ScriptedChooser::new(names)),
                None => Box::new(UniformChooser),
            };
            let mut walker = Walker {
                cat: &cat,
                chooser,
                proposer: NullProposer,
                threshold,
            };
            let walk = walker.walk(&goal, from, steps);
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
                            Escalation::NoneOfThese => "System 1 chose none-of-these",
                            Escalation::LowConfidence => "System 1 below threshold",
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
        Cmd::Run(_) => unreachable!("handled in main"),
    }
    Ok(ExitCode::SUCCESS)
}
