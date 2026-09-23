//! `onto laws`: what entry contracts and arrow effects imply.

use std::collections::BTreeSet;
use std::path::PathBuf;

use clap::Args;
use onto_core::laws::{derive, enforced};
use onto_core::{Invariant, ObjId};

use crate::run::BoxError;

#[derive(Args)]
pub struct LawsArgs {
    file: PathBuf,
    /// Start objects, comma separated (default: objects with no incoming arrow).
    #[arg(long, value_delimiter = ',')]
    from: Vec<String>,
}

pub fn main(args: LawsArgs) -> Result<(), BoxError> {
    let src =
        std::fs::read_to_string(&args.file).map_err(|e| format!("{}: {e}", args.file.display()))?;
    let cat = onto_core::parse(&src)?;
    let starts = args
        .from
        .iter()
        .map(|n| cat.object_id(n))
        .collect::<Result<Vec<_>, _>>()?;
    let laws = derive(&cat, &starts);
    let name = |o: ObjId| cat.object(o).name.as_str();
    let arrow = |a: onto_core::ArrowId| cat.arrow(a).name.as_str();
    let set = |t: &BTreeSet<String>| {
        if t.is_empty() {
            "—".to_owned()
        } else {
            t.iter().cloned().collect::<Vec<_>>().join(", ")
        }
    };

    println!(
        "category {} · walks start at {} holding no tokens · case preconditions assumed satisfiable",
        cat.name(),
        laws.starts
            .iter()
            .map(|o| name(*o))
            .collect::<Vec<_>>()
            .join(", ")
    );

    let tokens: BTreeSet<String> = cat
        .arrows()
        .iter()
        .flat_map(|a| a.ensures.iter().chain(&a.revokes))
        .chain(cat.objects().iter().flat_map(|o| &o.entry.needs))
        .cloned()
        .collect();
    if tokens.is_empty() {
        println!("\nno capability tokens declared (no `ensures`, `revokes` or `entry … needs`)");
    } else {
        println!("\ntokens");
        for t in &tokens {
            let by = |f: fn(&onto_core::Arrow) -> &Vec<String>| {
                let v: Vec<&str> = cat
                    .arrows()
                    .iter()
                    .filter(|a| f(a).contains(t))
                    .map(|a| a.name.as_str())
                    .collect();
                if v.is_empty() {
                    "—".to_owned()
                } else {
                    v.join(", ")
                }
            };
            let needed: Vec<&str> = cat
                .objects()
                .iter()
                .filter(|o| o.entry.needs.contains(t))
                .map(|o| o.name.as_str())
                .collect();
            println!(
                "  {t:<18} ensured by {} · revoked by {} · needed on entry to {}",
                by(|a| &a.ensures),
                by(|a| &a.revokes),
                if needed.is_empty() {
                    "—".to_owned()
                } else {
                    needed.join(", ")
                }
            );
        }
    }

    println!("\non arrival by an arrow (every walk holds · some walk may hold)");
    for o in &laws.reached {
        let must = laws.must(*o).unwrap_or_default();
        let entry = &cat.object(*o).entry;
        let contract = if entry.is_empty() {
            String::new()
        } else {
            let mut parts = Vec::new();
            if !entry.needs.is_empty() {
                parts.push(format!("needs {}", entry.needs.join(", ")));
            }
            if let Some(r) = &entry.require {
                parts.push(format!("require {r}"));
            }
            format!("   entry: {}", parts.join(" · "))
        };
        println!(
            "  {:<16} {:<28} {:<28}{contract}",
            name(*o),
            set(&must),
            set(&laws.may(*o))
        );
    }

    let mut derived = Vec::new();
    for o in laws.arrivals.keys() {
        let stated: BTreeSet<&String> = cat.object(*o).entry.needs.iter().collect();
        for t in laws.must(*o).unwrap_or_default() {
            let ensurers: Vec<&str> = cat
                .arrows()
                .iter()
                .filter(|a| a.ensures.contains(&t))
                .map(|a| a.name.as_str())
                .collect();
            let revokers: Vec<&str> = cat
                .arrows()
                .iter()
                .filter(|a| a.revokes.contains(&t))
                .map(|a| a.name.as_str())
                .collect();
            let since = if revokers.is_empty() {
                String::new()
            } else {
                format!(", and no {} since", revokers.join(" or "))
            };
            let kind = if stated.contains(&t) {
                "by entry contract"
            } else {
                "derived"
            };
            derived.push(format!(
                "  every walk into {} has taken {}{since}   ({kind}: holds {t})",
                name(*o),
                ensurers.join(" or "),
            ));
        }
    }
    println!("\nlaws");
    if derived.is_empty() {
        println!("  none: no object is reached only by token-holding walks");
    }
    for l in derived {
        println!("{l}");
    }

    let dead = laws.dead(&cat);
    println!("\ndead arrows (source reachable, never walkable)");
    if dead.is_empty() {
        println!("  none");
    }
    for a in dead {
        let x = cat.arrow(a);
        let dst = cat.object(x.dst);
        println!(
            "  {}: {} -> {}   {} needs {} on entry",
            arrow(a),
            name(x.src),
            dst.name,
            dst.name,
            dst.entry.needs.join(", ")
        );
    }

    let vias: Vec<&Invariant> = cat
        .invariants()
        .iter()
        .filter(|i| matches!(i, Invariant::Via { .. }))
        .collect();
    if !vias.is_empty() {
        println!("\ninvariants   graph: no path bypasses · walks: no walk can bypass (contracts)");
        for inv in vias {
            let walks = match enforced(&cat, inv) {
                Some(true) => "✓",
                Some(false) => "✗",
                None => "?",
            };
            println!("  {inv:<62} graph ✓   walks {walks}");
        }
    }
    Ok(())
}
