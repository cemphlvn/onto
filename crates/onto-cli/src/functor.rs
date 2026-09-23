//! `onto functor`: what a functor keeps, reflects and leaves uncovered.

use std::path::PathBuf;

use clap::Args;
use onto_core::equality::Verdict;
use onto_core::{Functor, Obligation};

use crate::run::BoxError;

#[derive(Args)]
pub struct FunctorArgs {
    /// A file declaring (or importing) the categories and the functor.
    file: PathBuf,
    /// The functor (default: every functor the file loads).
    name: Option<String>,
}

pub fn main(args: FunctorArgs) -> Result<(), BoxError> {
    let (module, _) = crate::module::load_module(&args.file)?;
    let chosen: Vec<&Functor> = module
        .functors
        .iter()
        .filter(|f| args.name.as_deref().is_none_or(|n| f.name == n))
        .collect();
    if chosen.is_empty() {
        return Err("no such functor".into());
    }
    for f in chosen {
        let a = module.category(&f.src).expect("loaded");
        let b = module.category(&f.dst).expect("loaded");
        let r = &f.report;
        let domain = f.objects.iter().flatten().count();
        println!(
            "functor {}: {} -> {}   domain {domain} of {} objects · {} arrows mapped",
            f.name,
            f.src,
            f.dst,
            a.objects().len(),
            f.arrows.iter().flatten().count()
        );
        if !r.equations.is_empty() {
            let unknown = r
                .equations
                .iter()
                .filter(|(_, v)| *v == Verdict::Unknown)
                .count();
            println!(
                "  equations kept: {} ✓{}",
                r.equations.len() - unknown,
                if unknown > 0 {
                    format!(" · {unknown} unknown (limits reached)")
                } else {
                    String::new()
                }
            );
        }
        println!(
            "\n  reflected in {} (required ones are proved at load)",
            f.src
        );
        for (o, v) in [
            (Obligation::Authority, &r.authority),
            (Obligation::Contracts, &r.contracts),
            (Obligation::Invariants, &r.invariants),
        ] {
            let req = if f.require.contains(&o) {
                "required"
            } else {
                "reported"
            };
            if v.is_empty() {
                println!("    {:<11} ✓   ({req})", o.as_str());
            } else {
                println!("    {:<11} ✗   ({req})", o.as_str());
                for x in v {
                    println!("      {x}");
                }
            }
        }
        for x in &r.not_preserved {
            println!("    not preserved: {x}");
        }
        if !r.changed_frames.is_empty() {
            println!(
                "\n  frames whose options changed (their earlier answers are to a different question)"
            );
            for c in &r.changed_frames {
                println!("    {c}");
            }
        }
        if !r.relabelled.is_empty() {
            println!("\n  same structure, changed meaning");
            for c in &r.relabelled {
                println!("    {c}");
            }
        }
        println!("\n  {} arrows, backed by", f.dst);
        let width = r.backing.iter().map(|(g, _)| g.len()).max().unwrap_or(0);
        for (g, by) in &r.backing {
            let all = by.iter().all(|(_, att)| *att);
            let list: Vec<String> = by
                .iter()
                .map(|(n, att)| format!("{n}{}", if *att { " (attested)" } else { "" }))
                .collect();
            println!(
                "    {g:<width$}  {}{}",
                list.join(", "),
                if all { "   · all evidence signed" } else { "" }
            );
        }
        if r.uncovered_arrows.is_empty() && r.uncovered_objects.is_empty() {
            println!(
                "\n  coverage: every object and arrow of {} has a preimage",
                f.dst
            );
        } else {
            println!("\n  not covered (no preimage in {}):", f.src);
            if !r.uncovered_objects.is_empty() {
                println!("    objects  {}", r.uncovered_objects.join(", "));
            }
            if !r.uncovered_arrows.is_empty() {
                let shown: Vec<String> = r
                    .uncovered_arrows
                    .iter()
                    .map(|g| {
                        let x = b.arrow(b.arrow_id(g).expect("own arrow"));
                        format!("{g}: {} -> {}", b.object(x.src).name, b.object(x.dst).name)
                    })
                    .collect();
                println!("    arrows   {}", shown.join(" · "));
            }
        }
        if f.transport {
            println!("\n  transport: on (empty fibers become proposals when a walk escalates)");
        }
        println!();
    }
    Ok(())
}
