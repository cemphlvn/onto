//! `onto curate`: one proposal per gap, for a person to review.
//!
//! Runs write gap signals (stops that new structure could address but not
//! on the case's path) instead of making cases wait. Curation groups them
//! by gap, asks the proposer once per gap with a few representative cases
//! (as their frame's state policy showed them), and writes records that
//! `onto review` reads; `onto promote` stays the human step.

use std::io::Write;
use std::path::PathBuf;

use clap::Args;
use onto_runtime::curation::{GapSignal, group, request};
use onto_runtime::model::Proposer;
use serde_json::json;

use crate::run::{BoxError, models};

#[derive(Args)]
pub struct CurateArgs {
    /// The category (`FILE` or `FILE#Name`).
    file: PathBuf,
    /// Gap signals (default: `<file stem>.gaps.jsonl`).
    #[arg(long)]
    gaps: Option<PathBuf>,
    /// Records for `onto review` (default: `<file stem>.curated.jsonl`).
    #[arg(long)]
    out: Option<PathBuf>,
    /// Representative cases shown per gap.
    #[arg(long, default_value_t = 3)]
    examples: usize,
    /// Offline mock proposer.
    #[arg(long)]
    mock: bool,
}

pub fn main(args: CurateArgs) -> Result<(), BoxError> {
    let cat = crate::module::load_category(&args.file)?;
    let (file, _) = crate::module::target(&args.file);
    let gaps = args
        .gaps
        .clone()
        .unwrap_or_else(|| file.with_extension("gaps.jsonl"));
    let text = std::fs::read_to_string(&gaps).map_err(|e| format!("{}: {e}", gaps.display()))?;
    let signals: Vec<GapSignal> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    let groups = group(signals);
    println!(
        "{}: {} signal(s) in {} gap(s)",
        gaps.display(),
        groups.iter().map(|g| g.signals.len()).sum::<usize>(),
        groups.len()
    );
    let (_, proposer) = models(args.mock, args.mock, None, None)?;
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let results = rt.block_on(async {
        let mut set = tokio::task::JoinSet::new();
        let proposer = std::sync::Arc::new(proposer);
        for (i, g) in groups.iter().enumerate() {
            let req = request(&cat, g, args.examples);
            let p = proposer.clone();
            set.spawn(async move {
                match req {
                    Ok(r) => (
                        i,
                        p.propose(r).await.map(|x| x.0).map_err(|e| e.to_string()),
                    ),
                    Err(e) => (i, Err(e.to_string())),
                }
            });
        }
        let mut out = vec![None; groups.len()];
        while let Some(Ok((i, r))) = set.join_next().await {
            out[i] = Some(r);
        }
        out
    });
    let out_path = args
        .out
        .clone()
        .unwrap_or_else(|| file.with_extension("curated.jsonl"));
    let mut out = std::fs::File::create(&out_path)?;
    for (i, (g, r)) in groups.iter().zip(results).enumerate() {
        let cases = {
            let mut c: Vec<&Option<String>> = g.signals.iter().map(|s| &s.case).collect();
            c.sort();
            c.dedup();
            c.len()
        };
        println!();
        println!(
            "  gap {} · {} · {} signal(s) from {cases} case(s)",
            i + 1,
            g.key,
            g.signals.len()
        );
        let proposals = match r {
            Some(Ok(p)) => p,
            Some(Err(e)) => {
                println!("    proposer failed: {e}");
                continue;
            }
            None => continue,
        };
        for p in &proposals {
            println!(
                "    proposed {}: {} -> {}  — {}",
                p.arrow, p.src, p.dst, p.about
            );
        }
        writeln!(
            out,
            "{}",
            serde_json::to_string(&json!({
                "id": format!("gap{}", i + 1),
                "snapshot": cat.snapshot(),
                "gap": g.key,
                "cases": cases,
                "records": g.signals.iter().map(|s| &s.record).collect::<Vec<_>>(),
                "proposals": proposals,
            }))?
        )?;
    }
    println!();
    println!(
        "curated: {} (next: onto review {} {})",
        out_path.display(),
        args.file.display(),
        out_path.display()
    );
    Ok(())
}
