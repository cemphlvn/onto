//! `onto laws`: what entry contracts and arrow effects imply, with proofs.

use std::collections::BTreeSet;
use std::path::PathBuf;

use clap::Args;
use onto_core::laws::{Proof, derive, enforced, startable};
use onto_core::{ArrowId, Category, Invariant, ObjId};

use crate::run::BoxError;

#[derive(Args)]
pub struct LawsArgs {
    file: PathBuf,
    /// Guarantees made by this application: walks start at the declared
    /// `start:` objects (the default when any are declared).
    #[arg(long, conflicts_with_all = ["all_startable", "from"])]
    from_declared_roots: bool,
    /// Guarantees made by the category itself: walks may start at any
    /// object whose entry contract needs no tokens (the default otherwise).
    #[arg(long, conflicts_with = "from")]
    all_startable: bool,
    /// Start at these objects, comma separated.
    #[arg(long, value_delimiter = ',')]
    from: Vec<String>,
    /// Print a witness or counterexample path for every claim.
    #[arg(long)]
    proofs: bool,
}

pub fn main(args: LawsArgs) -> Result<(), BoxError> {
    let src =
        std::fs::read_to_string(&args.file).map_err(|e| format!("{}: {e}", args.file.display()))?;
    let cat = onto_core::parse(&src)?;
    let (starts, mode) = if !args.from.is_empty() {
        let ids = args
            .from
            .iter()
            .map(|n| cat.object_id(n))
            .collect::<Result<Vec<_>, _>>()?;
        (ids, "given starts")
    } else if args.from_declared_roots || (!args.all_startable && !cat.starts().is_empty()) {
        if cat.starts().is_empty() {
            return Err("no `start:` declared in this category".into());
        }
        (
            cat.starts().to_vec(),
            "declared roots: guarantees of this application",
        )
    } else {
        (
            startable(&cat),
            "all startable objects: guarantees of the category itself",
        )
    };
    let laws = derive(&cat, &starts);
    let name = |o: ObjId| cat.object(o).name.as_str();
    let set = |t: &BTreeSet<String>| {
        if t.is_empty() {
            "—".to_owned()
        } else {
            t.iter().cloned().collect::<Vec<_>>().join(", ")
        }
    };

    println!("category {} · {mode}", cat.name());
    println!(
        "walks start at {} holding no tokens · case preconditions assumed satisfiable · {} states explored",
        laws.starts
            .iter()
            .map(|o| name(*o))
            .collect::<Vec<_>>()
            .join(", "),
        laws.states.values().map(BTreeSet::len).sum::<usize>(),
    );

    if cat.capabilities().is_empty() {
        println!("\nno capabilities declared");
    } else {
        println!("\ncapabilities   authorized (declared) · used by some walk");
        for c in cat.capabilities() {
            let used = |names: &[String]| -> String {
                let v: Vec<&str> = names
                    .iter()
                    .filter(|n| cat.arrow_id(n).is_ok_and(|a| laws.used.contains(&a)))
                    .map(String::as_str)
                    .collect();
                if v.is_empty() {
                    "—".into()
                } else {
                    v.join(", ")
                }
            };
            let needed: Vec<&str> = cat
                .objects()
                .iter()
                .filter(|o| o.entry.needs.contains(&c.name))
                .map(|o| o.name.as_str())
                .collect();
            println!("  {}", c.name);
            println!(
                "    issuers   {} · used {}",
                or_none(&c.issuers),
                used(&c.issuers)
            );
            println!(
                "    revokers  {} · used {}",
                or_none(&c.revokers),
                used(&c.revokers)
            );
            println!(
                "    needed on entry to {}",
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
        let (must, may) = match laws.must(*o) {
            Some(m) => (set(&m), set(&laws.may(*o))),
            None => ("(started only)".to_owned(), "—".to_owned()),
        };
        println!("  {:<16} {must:<28} {may:<28}{contract}", name(*o));
    }

    println!("\nlaws (each with its certificate)");
    let mut any = false;
    for o in laws.arrivals.keys() {
        let stated: BTreeSet<&String> = cat.object(*o).entry.needs.iter().collect();
        for t in laws.may(*o) {
            match laws.prove(&cat, *o, &t) {
                Some(Proof::Must {
                    arrival_states,
                    granted_by,
                    revoked_by,
                }) => {
                    any = true;
                    let kind = if stated.contains(&t) {
                        "by entry contract"
                    } else {
                        "derived"
                    };
                    println!("  MUST  every walk into {} holds {t}   ({kind})", name(*o));
                    println!(
                        "        certificate: {arrival_states} arrival state(s), 0 without {t}; granted only by {}{}",
                        arrows(&cat, &granted_by),
                        if revoked_by.is_empty() {
                            String::new()
                        } else {
                            format!(
                                "; after {} it is re-granted before arrival",
                                arrows(&cat, &revoked_by)
                            )
                        },
                    );
                    if args.proofs {
                        for tokens in &laws.arrivals[o] {
                            if let Some(p) = laws.path_to(*o, tokens) {
                                println!("        · {}", show(&cat, &p));
                            }
                        }
                    }
                }
                Some(Proof::NotAlways { counterexample }) if args.proofs => {
                    println!(
                        "  MAY   some walk into {} holds {t}, not every walk",
                        name(*o)
                    );
                    if let Some(w) = laws.witness(*o, &t) {
                        println!("        witness:        {}", show(&cat, &w));
                    }
                    println!("        counterexample: {}", show(&cat, &counterexample));
                }
                _ => {}
            }
        }
    }
    if !any {
        println!("  none: no object is reached only by token-holding walks");
    }

    let dead = laws.dead(&cat);
    println!("\ndead arrows (source reachable, never walkable)");
    if dead.is_empty() {
        println!("  none");
    }
    for a in dead {
        let x = cat.arrow(a);
        let dst = cat.object(x.dst);
        let held: Vec<String> = laws
            .states
            .get(&x.src)
            .into_iter()
            .flatten()
            .map(|t| format!("{{{}}}", set(t)))
            .collect();
        println!(
            "  {}: {} -> {}   {} needs {} on entry; {} is only ever held with {}",
            x.name,
            name(x.src),
            dst.name,
            dst.name,
            dst.entry.needs.join(", "),
            name(x.src),
            held.join(" or ")
        );
    }

    let vias: Vec<&Invariant> = cat
        .invariants()
        .iter()
        .filter(|i| matches!(i, Invariant::Via { .. }))
        .collect();
    if !vias.is_empty() {
        println!("\ninvariants   graph: proved at load · walks: no walk can bypass (contracts)");
        for inv in vias {
            let walks = match enforced(&cat, inv) {
                Some(true) => "✓",
                Some(false) => "✗",
                None => "?",
            };
            println!("  {inv:<62} graph ✓   walks {walks}");
        }
    }
    print_seen(&cat);
    Ok(())
}

/// What each frame shows a model (the `state` policy), and the `unseen`
/// invariants it proves.
fn print_seen(cat: &Category) {
    use onto_core::Primitive;
    println!(
        "
what models see   per the `state` policy · case and observed fields proved · goal is free text"
    );
    if !cat.declares_state() {
        println!(
            "  no `state` declared: every frame sees the goal, the whole case, history, focus and tokens"
        );
    }
    let (mut asked, mut never) = (Vec::new(), Vec::new());
    for i in 0..cat.objects().len() as u32 {
        let o = ObjId(i);
        let x = cat.object(o);
        let closed = x.closure == onto_core::Closure::Closed;
        let terminal = closed && cat.out(o).is_empty();
        let split = x.frame.primitive == Primitive::Split && closed;
        if terminal || split {
            never.push(x.name.as_str());
            continue;
        }
        let who = if cat.out(o).is_empty() || x.frame.primitive == Primitive::Split {
            "proposer"
        } else {
            "judge"
        };
        asked.push((x.name.as_str(), who, cat.state_of(o)));
    }
    let width = asked.iter().map(|(n, ..)| n.len()).max().unwrap_or(0);
    for (name, who, spec) in &asked {
        println!("  {name:<width$}  {who:<8}  {}", spec.summary());
    }
    if !never.is_empty() {
        println!(
            "  never asked (terminal or closed split): {}",
            never.join(", ")
        );
    }
    let goal_frames: Vec<&str> = asked
        .iter()
        .filter(|(_, _, s)| s.goal)
        .map(|(n, ..)| *n)
        .collect();
    for inv in cat.invariants() {
        if let Invariant::Unseen(_) = inv {
            println!(
                "  {inv}   ✓ proved: no frame shows these fields, nor any field inside or around them"
            );
        }
    }
    if !goal_frames.is_empty() {
        println!(
            "  goal (free text) reaches {}: its content is whatever the requester wrote, not covered by `unseen`",
            goal_frames.join(", ")
        );
    }
}

fn or_none(v: &[String]) -> String {
    if v.is_empty() {
        "—".into()
    } else {
        v.join(", ")
    }
}

fn arrows(cat: &Category, v: &[ArrowId]) -> String {
    if v.is_empty() {
        "—".into()
    } else {
        v.iter()
            .map(|a| cat.arrow(*a).name.as_str())
            .collect::<Vec<_>>()
            .join(" or ")
    }
}

/// `Collected --consent [+ConsentGrant +LegalBasis]--> Consented --pseudonymize--> …`
fn show(cat: &Category, path: &[ArrowId]) -> String {
    let Some(first) = path.first() else {
        return "(start)".into();
    };
    let mut out = cat.object(cat.arrow(*first).src).name.clone();
    for a in path {
        let x = cat.arrow(*a);
        let effects: Vec<String> = x
            .revokes
            .iter()
            .map(|t| format!("-{t}"))
            .chain(x.ensures.iter().map(|t| format!("+{t}")))
            .collect();
        let effects = if effects.is_empty() {
            String::new()
        } else {
            format!(" [{}]", effects.join(" "))
        };
        out.push_str(&format!(
            " --{}{effects}--> {}",
            x.name,
            cat.object(x.dst).name
        ));
    }
    out
}
