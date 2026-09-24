//! `onto discover SRC TGT`: a candidate functor from one category into
//! another (`docs/05-functors.md` §7).
//!
//! 1. Structure: the targets each source object's role allows.
//! 2. Meaning: per source object, the judge chooses among those (or none).
//! 3. Behaviour (`--runs A.telemetry,B.telemetry`): the same cases walked
//!    in both categories; objects that the same cases visit score higher.
//! 4. Search: the best object map under which every arrow has a path.
//! 5. The functor checks, as for a hand-written functor.
//!
//! The output is a declaration to review, never adopted automatically: a
//! functor is policy (it enables transport, views, ensemble columns).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use clap::Args;
use onto_core::ObjId;
use onto_core::discover;
use onto_core::functor::Functor;
use serde_json::Value;

use crate::run::{BoxError, models};

#[derive(Args)]
pub struct DiscoverArgs {
    /// The source category (`FILE#Name`).
    src: PathBuf,
    /// The target category (`FILE#Name`).
    dst: PathBuf,
    /// The functor's name.
    #[arg(long, default_value = "Discovered")]
    name: String,
    /// Telemetry of runs over the same cases (case ids), source then
    /// target: behavioural evidence.
    #[arg(long, value_delimiter = ',', num_args = 1..)]
    runs: Vec<PathBuf>,
    /// Structure (and behaviour) only: no judge.
    #[arg(long)]
    no_judge: bool,
    #[arg(long)]
    mock: bool,
    /// Longest target path an arrow may map to.
    #[arg(long, default_value_t = 3)]
    max_len: usize,
    /// Let two source objects share an image.
    #[arg(long)]
    non_injective: bool,
    /// A hand-written functor (`FILE#Name`) to compare with.
    #[arg(long)]
    compare: Option<PathBuf>,
    /// Write the declaration here (a proposal to review).
    #[arg(long)]
    out: Option<PathBuf>,
}

pub fn main(args: DiscoverArgs) -> Result<(), BoxError> {
    let a = crate::module::load_category(&args.src)?;
    let b = crate::module::load_category(&args.dst)?;
    let n = a.objects().len();
    let adm = discover::admissible(&a, &b);
    let pairs: usize = adm.iter().map(Vec::len).sum();
    println!(
        "discover {}: {} -> {}   structure admits {pairs} of {} object pairs",
        args.name,
        a.name(),
        b.name(),
        n * b.objects().len()
    );

    // Meaning.
    let judged = if args.no_judge {
        vec![None; n]
    } else {
        let (judge, _) = models(args.mock, true, None, None)?;
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;
        println!(
            "judge {}: {} object(s) with more than one admissible target",
            onto_runtime::model::Judge::name(&judge),
            adm.iter().filter(|v| v.len() > 1).count()
        );
        rt.block_on(onto_runtime::discovery::judge_objects(&judge, &a, &b, &adm))?
    };

    // Behaviour.
    let behaviour = match args.runs.as_slice() {
        [] => None,
        [ra, rb] => Some((visits(ra)?, visits(rb)?)),
        _ => return Err("--runs takes two telemetry files: source, target".into()),
    };
    let jaccard = |x: ObjId, y: ObjId| -> Option<(f32, usize)> {
        let (va, vb) = behaviour.as_ref()?;
        let ca = cases_at(va, &a.object(x).name);
        let cb = cases_at(vb, &b.object(y).name);
        if ca.is_empty() && cb.is_empty() {
            return None;
        }
        let both = ca.intersection(&cb).count();
        let either = ca.union(&cb).count();
        Some((both as f32 / either as f32, both))
    };

    // Scores: the mean of the signals present for the pair.
    let mut scores: Vec<Vec<(ObjId, f32)>> = Vec::with_capacity(n);
    let mut outside = vec![0.05f32; n];
    for (i, ys) in adm.iter().enumerate() {
        let x = ObjId(i as u32);
        let mut row = Vec::new();
        for y in ys {
            let mut s = Vec::new();
            match &judged[i] {
                Some(j) => s.push(j.targets.iter().find(|t| t.0 == *y).map_or(0.0, |t| t.1)),
                None if ys.len() == 1 => s.push(1.0),
                None => {}
            }
            if let Some((jac, _)) = jaccard(x, *y) {
                s.push(jac);
            }
            let score = if s.is_empty() {
                0.5
            } else {
                s.iter().sum::<f32>() / s.len() as f32
            };
            row.push((*y, score));
        }
        if let Some(j) = &judged[i] {
            outside[i] = j.none;
        }
        scores.push(row);
    }

    let found = discover::search(&a, &b, &scores, &outside, !args.non_injective, args.max_len);
    let images = discover::arrow_images(&a, &b, &found.objects, args.max_len);
    let arrows: Vec<Option<onto_core::path::Path>> = images
        .iter()
        .map(|v| v.as_ref().and_then(|v| v.first().cloned()))
        .collect();
    let decl = discover::declaration(&args.name, &a, &b, &found.objects, &arrows);

    // Provenance per mapped object.
    let note = |x: &str| -> Option<String> {
        let xi = a.object_id(x).ok()?;
        let i = xi.0 as usize;
        let y = found.objects[i]?;
        let mut parts = Vec::new();
        match &judged[i] {
            Some(j) => {
                let p = j.targets.iter().find(|t| t.0 == y).map_or(0.0, |t| t.1);
                parts.push(format!("judged {p:.2}"));
            }
            None if adm[i].len() == 1 => parts.push("structure: the only admissible target".into()),
            None => {}
        }
        if let Some((jac, both)) = jaccard(xi, y) {
            parts.push(format!("co-visited {jac:.2} ({both} cases)"));
        }
        Some(parts.join(" · "))
    };
    let text = discover::render(&decl, &note);
    println!();
    print!("{text}");
    let outside_names: Vec<String> = found
        .objects
        .iter()
        .enumerate()
        .filter(|(_, y)| y.is_none())
        .map(|(i, _)| {
            let why = judged[i]
                .as_ref()
                .map_or(String::new(), |j| format!(" (none of these {:.2})", j.none));
            format!("{}{why}", a.objects()[i].name)
        })
        .collect();
    if !outside_names.is_empty() {
        println!("outside the domain: {}", outside_names.join(", "));
    }
    let ambiguous: Vec<String> = images
        .iter()
        .enumerate()
        .filter_map(|(i, v)| {
            let v = v.as_ref()?;
            (v.len() > 1).then(|| format!("{} ({} shortest paths)", a.arrows()[i].name, v.len()))
        })
        .collect();
    if !ambiguous.is_empty() {
        println!(
            "ambiguous arrows (first path taken; review): {}",
            ambiguous.join(", ")
        );
    }
    println!(
        "search: score {:.2} · {} assignments tried · {} rejected because an arrow had no path",
        found.score, found.explored, found.pruned
    );

    // The functor checks.
    println!();
    match Functor::build(&decl, &a, &b) {
        Ok(f) => {
            let r = &f.report;
            let show = |label: &str, v: &[String]| {
                if v.is_empty() {
                    println!("  {label:<11} ✓");
                } else {
                    println!("  {label:<11} ✗");
                    for x in v {
                        println!("      {x}");
                    }
                }
            };
            println!("checks (as for a hand-written functor):");
            show("authority", &r.authority);
            show("contracts", &r.contracts);
            show("invariants", &r.invariants);
            if !r.uncovered_objects.is_empty() {
                println!(
                    "  not covered in {}: {}",
                    b.name(),
                    r.uncovered_objects.join(", ")
                );
            }
        }
        Err(e) => println!("checks: the declaration does not load: {e}"),
    }

    if let Some(spec) = &args.compare {
        let (file, name) = crate::module::target(spec);
        let (module, _) = crate::module::load_module(&file)?;
        let name = name.ok_or("--compare needs FILE#Functor")?;
        let hand = module
            .functor(&name)
            .ok_or_else(|| format!("{}: no functor {name}", file.display()))?;
        println!();
        println!("compared with {name} (hand-written):");
        let mut same = 0;
        for (i, (h, d)) in hand.objects.iter().zip(&found.objects).enumerate() {
            let show = |o: &Option<ObjId>| o.map_or("—".to_owned(), |y| b.object(y).name.clone());
            if h == d {
                same += 1;
            } else {
                println!(
                    "  {:<16} hand {:<14} discovered {}",
                    a.objects()[i].name,
                    show(h),
                    show(d)
                );
            }
        }
        println!("  {same} of {n} objects map the same way");
        // Behaviour against the adopted map: pairs the same cases rarely
        // share. The functor may be sound and the practice still differ.
        if behaviour.is_some() {
            let weak: Vec<String> = hand
                .objects
                .iter()
                .enumerate()
                .filter_map(|(i, y)| {
                    let (x, y) = (ObjId(i as u32), (*y)?);
                    let (jac, both) = jaccard(x, y)?;
                    (jac < 0.34).then(|| {
                        format!(
                            "{} -> {} co-visited {jac:.2} ({both} shared cases)",
                            a.objects()[i].name,
                            b.object(y).name
                        )
                    })
                })
                .collect();
            if !weak.is_empty() {
                println!(
                    "  behaviour does not support (the organisations handle these cases differently):"
                );
                for w in weak {
                    println!("    {w}");
                }
            }
        }
    }

    if let Some(path) = &args.out {
        let header = format!(
            "# Discovered by `onto discover` ({} -> {}): a proposal to review.\n# Adopting it is a person's act: copy it into the policy file.\n\n",
            a.name(),
            b.name()
        );
        std::fs::write(path, header + &text)?;
        println!();
        println!("written to {}", path.display());
    }
    Ok(())
}

/// Case → objects its walks visited, from a telemetry file.
fn visits(path: &PathBuf) -> Result<BTreeMap<String, BTreeSet<String>>, BoxError> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let events: Vec<Value> = text
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    let case_of: BTreeMap<u64, String> = events
        .iter()
        .filter(|e| e["event"] == "walk.start")
        .filter_map(|e| Some((e["walk"].as_u64()?, e["case"].as_str()?.to_owned())))
        .collect();
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for e in &events {
        let Some(case) = e["walk"].as_u64().and_then(|w| case_of.get(&w)) else {
            continue;
        };
        let objs = out.entry(case.clone()).or_default();
        match e["event"].as_str() {
            Some("walk.start") => {
                if let Some(f) = e["from"].as_str() {
                    objs.insert(f.to_owned());
                }
            }
            Some("step") => {
                for k in ["from", "to"] {
                    if let Some(o) = e[k].as_str() {
                        objs.insert(o.to_owned());
                    }
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

fn cases_at(v: &BTreeMap<String, BTreeSet<String>>, object: &str) -> BTreeSet<String> {
    v.iter()
        .filter(|(_, objs)| objs.contains(object))
        .map(|(c, _)| c.clone())
        .collect()
}
