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
use onto_core::calibrate;
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
    /// The judge, as for `onto run`: a Jev model name, `local:<file.gguf>`
    /// (feature `local`), or `tkgd:<model>`. Default: Jev.
    #[arg(long)]
    judge_model: Option<String>,
    /// Longest target path an arrow may map to.
    #[arg(long, default_value_t = 3)]
    max_len: usize,
    /// Let two source objects share an image.
    #[arg(long)]
    non_injective: bool,
    /// No structural roles (entry, terminal, primitive): every target is
    /// admissible. For categories that are not walked, such as
    /// dependency graphs; arrows still need a path.
    #[arg(long)]
    no_roles: bool,
    /// Judge calls in flight at once.
    #[arg(long, default_value_t = 16)]
    concurrency: usize,
    /// What the correspondence is for, told to the judge; `{src}`,
    /// `{dst}`, `{object}` are filled in. Default: two organisations'
    /// case handling.
    #[arg(long)]
    goal: Option<String>,
    /// The judge's question, same placeholders as `--goal`.
    #[arg(long)]
    instructions: Option<String>,
    /// The share of judge calls that may fail (after retries) before the
    /// discovery stops. Objects whose call failed stay outside the
    /// domain and are named; no evidence is not guessed at.
    #[arg(long, default_value_t = 0.05)]
    max_failed: f32,
    /// The calibration map for the judge's p (`onto calibrate --out`).
    #[arg(long)]
    calibration: Option<PathBuf>,
    /// The cost of adopting a wrong correspondence, in the unit of
    /// `--cost-ask`. With it the gate is on: an object is adopted only
    /// when its calibrated p ≥ 1 − cost_ask / cost_wrong; below, it is
    /// deferred to a person, never guessed at.
    #[arg(long)]
    cost_wrong: Option<f64>,
    /// The cost of asking a person about one object.
    #[arg(long, default_value_t = 1.0)]
    cost_ask: f64,
    /// A person's answers: one `{"object": .., "target": ..}` per line
    /// (`target` null for none of them). Answered objects are pinned
    /// before the judge is asked.
    #[arg(long)]
    answers: Option<PathBuf>,
    /// Write one decision record per source object here (JSON lines):
    /// disposition, target, raw and calibrated p, threshold, candidates.
    #[arg(long)]
    records: Option<PathBuf>,
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
    let mut adm = if args.no_roles {
        discover::unconstrained(&a, &b)
    } else {
        discover::admissible(&a, &b)
    };
    let mut failed: Vec<Option<String>> = vec![None; n];
    // A person's answers pin their objects before anything is judged.
    let mut answered = vec![false; n];
    if let Some(path) = &args.answers {
        for (object, target) in read_answers(path)? {
            let x = a
                .object_id(&object)
                .map_err(|_| format!("{}: no object {object} in {}", path.display(), a.name()))?;
            let i = x.0 as usize;
            answered[i] = true;
            adm[i] =
                match target {
                    Some(t) => vec![b.object_id(&t).map_err(|_| {
                        format!("{}: no object {t} in {}", path.display(), b.name())
                    })?],
                    None => Vec::new(),
                };
        }
    }
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
        let (judge, _) = models(args.mock, true, args.judge_model.clone(), None)?;
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;
        println!(
            "judge {}: {} object(s) with more than one admissible target",
            onto_runtime::model::Judge::name(&judge),
            adm.iter().filter(|v| v.len() > 1).count()
        );
        let defaults = onto_runtime::discovery::Framing::default();
        let framing = onto_runtime::discovery::Framing {
            goal: args.goal.clone().unwrap_or(defaults.goal),
            instructions: args.instructions.clone().unwrap_or(defaults.instructions),
        };
        let out = rt.block_on(onto_runtime::discovery::judge_objects(
            &judge,
            &a,
            &b,
            &adm,
            args.concurrency,
            &framing,
        ));
        if !out.failed.is_empty() {
            let share = out.failed.len() as f32 / out.asked.max(1) as f32;
            if share > args.max_failed {
                return Err(format!(
                    "judge failed for {} of {} object(s) ({:.0}% > --max-failed {:.0}%); first: {}",
                    out.failed.len(),
                    out.asked,
                    share * 100.0,
                    args.max_failed * 100.0,
                    out.failed[0].1
                )
                .into());
            }
            println!(
                "judge failed for {} of {} object(s), kept outside the domain: {}",
                out.failed.len(),
                out.asked,
                out.failed
                    .iter()
                    .map(|(x, _)| a.object(*x).name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        // No evidence is not guessed at: an object whose call failed has
        // no admissible target, so the search leaves it outside.
        for (x, e) in &out.failed {
            adm[x.0 as usize].clear();
            failed[x.0 as usize] = Some(e.to_string());
        }
        out.judged
    };

    // The gate: calibrate the judge's p for its choice, defer what falls
    // below the threshold the costs give.
    let calibration = match &args.calibration {
        Some(path) => read_calibration(path)?,
        None => calibrate::Platt::identity(),
    };
    let threshold = args
        .cost_wrong
        .map(|w| calibrate::threshold(w, args.cost_ask));
    let mut gated: Vec<Option<(ObjId, f64, f64)>> = vec![None; n];
    for (i, j) in judged.iter().enumerate() {
        let Some(j) = j else { continue };
        let Some(&(y, p)) = j.targets.iter().max_by(|l, r| l.1.total_cmp(&r.1)) else {
            continue;
        };
        gated[i] = Some((y, f64::from(p), calibration.apply(f64::from(p))));
    }
    let mut deferred = vec![false; n];
    if let Some(t) = threshold {
        if args.calibration.is_none() {
            println!("gate: no --calibration, so it reads the judge's raw p");
        }
        for i in 0..n {
            if let Some((_, _, cal)) = gated[i]
                && cal < t
            {
                deferred[i] = true;
                adm[i].clear();
            }
        }
        let names: Vec<String> = (0..n)
            .filter(|i| deferred[*i])
            .map(|i| format!("{} ({:.2})", a.objects()[i].name, gated[i].unwrap().2))
            .collect();
        println!(
            "gate: adopt at calibrated p ≥ {t:.2} (cost wrong {} : ask {}); {} deferred to a person{}{}",
            args.cost_wrong.unwrap(),
            args.cost_ask,
            names.len(),
            if names.is_empty() { "" } else { ": " },
            names.join(", ")
        );
    }

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
            _ if answered[i] => parts.push("answered by a person".into()),
            Some(j) => {
                let p = j.targets.iter().find(|t| t.0 == y).map_or(0.0, |t| t.1);
                parts.push(format!("judged {p:.2}"));
                if args.calibration.is_some() {
                    parts.push(format!("calibrated {:.2}", calibration.apply(f64::from(p))));
                }
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
            let why = match (&judged[i], &failed[i]) {
                _ if answered[i] => " (answered: none of them)".to_owned(),
                _ if deferred[i] => format!(" (deferred, calibrated {:.2})", gated[i].unwrap().2),
                (Some(j), _) => format!(" (none of these {:.2})", j.none),
                (None, Some(_)) => " (judge failed)".to_owned(),
                (None, None) => String::new(),
            };
            format!("{}{why}", a.objects()[i].name)
        })
        .collect();
    if !outside_names.is_empty() {
        println!("outside the domain: {}", outside_names.join(", "));
    }

    // The decision records: what the next calibration is fitted on, once
    // a person has answered.
    if let Some(path) = &args.records {
        let mut out = String::new();
        for i in 0..n {
            let image = found.objects[i];
            let disposition = if answered[i] {
                "answered"
            } else if failed[i].is_some() {
                "failed"
            } else if deferred[i] {
                "deferred"
            } else if image.is_some() && judged[i].is_some() {
                "adopted"
            } else if image.is_some() {
                "structure"
            } else {
                "outside"
            };
            let target = image.or(gated[i].map(|g| g.0));
            let mut candidates: Vec<(ObjId, f32)> = judged[i]
                .as_ref()
                .map_or_else(Vec::new, |j| j.targets.clone());
            candidates.sort_by(|l, r| r.1.total_cmp(&l.1));
            let record = serde_json::json!({
                "object": a.objects()[i].name,
                "disposition": disposition,
                "target": target.map(|y| b.object(y).name.clone()),
                "p": gated[i].map(|g| g.1),
                "calibrated": gated[i].map(|g| g.2),
                "threshold": threshold,
                "none": judged[i].as_ref().map(|j| j.none),
                "candidates": candidates.iter().take(3)
                    .map(|(y, p)| serde_json::json!({"target": b.object(*y).name, "p": p}))
                    .collect::<Vec<_>>(),
            });
            out.push_str(&record.to_string());
            out.push('\n');
        }
        std::fs::write(path, out)?;
        println!("records: {n} decisions written to {}", path.display());
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
/// A person's answers: `{"object": .., "target": ..}` per line, `target`
/// null for none of them.
pub fn read_answers(path: &PathBuf) -> Result<Vec<(String, Option<String>)>, BoxError> {
    let text = std::fs::read_to_string(path)?;
    let mut out = Vec::new();
    for (k, line) in text
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
    {
        let v: Value =
            serde_json::from_str(line).map_err(|e| format!("{}:{}: {e}", path.display(), k + 1))?;
        let object = v["object"]
            .as_str()
            .ok_or_else(|| format!("{}:{}: no \"object\"", path.display(), k + 1))?;
        out.push((object.to_owned(), v["target"].as_str().map(str::to_owned)));
    }
    Ok(out)
}

/// A calibration map written by `onto calibrate`.
pub fn read_calibration(path: &PathBuf) -> Result<calibrate::Platt, BoxError> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(path)?)?;
    match v["method"].as_str() {
        Some("platt") => Ok(calibrate::Platt {
            a: v["a"].as_f64().ok_or("calibration: no \"a\"")?,
            b: v["b"].as_f64().ok_or("calibration: no \"b\"")?,
        }),
        other => Err(format!("{}: unknown calibration method {other:?}", path.display()).into()),
    }
}

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
