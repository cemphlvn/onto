//! `onto replay`: counterfactual questions to the judge.
//!
//! Every frame record keeps the exact state its model was shown (`seen`).
//! Replay asks the judge the same question again twice: once with that
//! state unchanged (the baseline: how stable is the model on identical
//! input?) and once with the state changed (`--drop` a field, `--set` one).
//! A decision that changes under the counterfactual but not under the
//! baseline depends on that field. Nothing is written; no walk moves.
//!
//! `--without-arrow A` asks the same question without A among the
//! candidates: the **presence effect** of a (learned) arrow. An arrow is
//! part of every judgment at its frame whether it is taken or not; this
//! measures what its presence did to the others.

use std::path::PathBuf;

use clap::Args;
use onto_core::walk::Answer;
use onto_core::{Category, Primitive};
use onto_runtime::model::{Candidate, FrameRequest, Judge};
use serde_json::Value;

use crate::run::{BoxError, models};

#[derive(Args)]
pub struct ReplayArgs {
    /// The category the records were made against.
    file: PathBuf,
    /// A disposition file (`onto run --dispositions`).
    dispositions: PathBuf,
    /// Replay these records (e.g. `w3.2`), comma separated.
    #[arg(long, value_delimiter = ',')]
    record: Vec<String>,
    /// Replay every judged visit at this frame.
    #[arg(long)]
    at: Option<String>,
    /// Remove a field from the state (`precedents`, `asserted.statement`).
    #[arg(long)]
    drop: Vec<String>,
    /// Replace a field: `asserted.statement=<JSON or text>`.
    #[arg(long)]
    set: Vec<String>,
    /// Ask without this arrow among the candidates (its presence effect).
    /// Without `--record`/`--at`: every judged record where it was a
    /// candidate.
    #[arg(long, value_name = "ARROW")]
    without_arrow: Option<String>,
    /// Ask with this arrow (from the learned layer, e.g. a dormant one)
    /// added to the candidates: would it have been taken? Without
    /// `--record`/`--at`: every judged record at its frame.
    #[arg(long, value_name = "ARROW", conflicts_with = "without_arrow")]
    with_arrow: Option<String>,
    /// The learned layer the records were made with (default:
    /// `<file stem>.learned.jsonl`); every entry, whatever its state.
    #[arg(long)]
    learned: Option<PathBuf>,
    /// The confidence gate the walk used (as `onto run --threshold`).
    #[arg(long, default_value_t = 0.6)]
    threshold: f32,
    #[arg(long)]
    mock: bool,
}

pub fn main(args: ReplayArgs) -> Result<(), BoxError> {
    if args.drop.is_empty()
        && args.set.is_empty()
        && args.without_arrow.is_none()
        && args.with_arrow.is_none()
    {
        return Err(
            "nothing to change: pass --drop FIELD, --set FIELD=VALUE or --without-arrow ARROW"
                .into(),
        );
    }
    let declared = crate::module::load_category(&args.file)?;
    let layer = args.learned.clone().unwrap_or_else(|| {
        let (file, name) = crate::module::target(&args.file);
        match name {
            Some(n) => file.with_extension(format!("{n}.learned.jsonl")),
            None => file.with_extension("learned.jsonl"),
        }
    });
    let entries = crate::learned::read(&layer)?;
    let proposals: Vec<_> = entries.iter().map(|l| l.proposal.clone()).collect();
    let (cat, _) = declared.with_learned(&proposals);
    let without = args.without_arrow.as_deref();
    let with = args.with_arrow.as_deref();
    let with_from: Option<String> = match with {
        Some(a) => Some(cat.object(cat.arrow(cat.arrow_id(a)?).src).name.clone()),
        None => None,
    };
    let text = std::fs::read_to_string(&args.dispositions)?;
    let records: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    let chosen: Vec<&Value> = records
        .iter()
        .filter(|r| r["judge"].is_object() && r["seen"].is_object())
        .filter(|r| {
            let judged = |a: &str| {
                r["candidates"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|c| c["arrow"] == a && !c["judgment"].is_null())
            };
            let named = args.record.iter().any(|id| r["id"] == id.as_str())
                || args.at.as_deref().is_some_and(|a| r["at"] == a);
            let at_with =
                || with_from.as_deref().is_some_and(|f| r["at"] == f) && !with.is_some_and(&judged);
            match (without, with) {
                (Some(a), _) if args.record.is_empty() && args.at.is_none() => judged(a),
                (Some(a), _) => named && judged(a),
                (_, Some(_)) if args.record.is_empty() && args.at.is_none() => at_with(),
                (_, Some(_)) => named && at_with(),
                _ => named,
            }
        })
        .collect();
    if chosen.is_empty() {
        return Err("no judged record with a recorded state matches".into());
    }
    let (judge, _) = models(args.mock, true, None, None)?;
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let change = without
        .map(|a| format!("without the arrow {a}"))
        .into_iter()
        .chain(with.map(|a| format!("with the arrow {a}")))
        .chain(args.drop.iter().map(|d| format!("without {d}")))
        .chain(args.set.iter().map(|s| format!("with {s}")))
        .collect::<Vec<_>>()
        .join(", ");
    println!("counterfactual: {change}   (judge {})", Judge::name(&judge));
    let (mut changed, mut unstable) = (0, 0);
    for r in &chosen {
        let (req, names) = request(&cat, r, r["seen"].clone())?;
        let mut cf_state = r["seen"].clone();
        for d in &args.drop {
            remove(&mut cf_state, d);
        }
        for s in &args.set {
            let (path, value) = s
                .split_once('=')
                .ok_or_else(|| format!("--set expects FIELD=VALUE, got `{s}`"))?;
            let value = serde_json::from_str(value).unwrap_or(Value::String(value.to_owned()));
            set(&mut cf_state, path, value);
        }
        let mut cf = req.clone();
        cf.state = cf_state;
        // The same question, without the arrow among the candidates.
        let kept: Vec<usize> = (0..names.len())
            .filter(|i| Some(names[*i].as_str()) != without)
            .collect();
        cf.candidates = kept.iter().map(|i| req.candidates[*i].clone()).collect();
        let mut kept_names: Vec<String> = kept.iter().map(|i| names[*i].clone()).collect();
        if let Some(a) = with {
            cf.candidates.push(Candidate::of(&cat, cat.arrow_id(a)?));
            kept_names.push(a.to_owned());
        }
        let (base, _) = rt.block_on(judge.judge(req))?;
        let (counter, _) = rt.block_on(judge.judge(cf))?;
        let recorded: Vec<Option<f64>> = names
            .iter()
            .map(|n| {
                r["candidates"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|c| c["arrow"] == n.as_str())
                    .and_then(|c| c["judgment"].as_f64())
            })
            .collect();
        let b = probs(&base);
        // Counterfactual judgments aligned with `names` (None: not asked).
        let mut c: Vec<Option<f32>> = vec![None; names.len()];
        for (k, i) in kept.iter().enumerate() {
            c[*i] = probs(&counter).get(k).copied();
        }
        let (db, dc) = (
            decision(&base, &names, args.threshold),
            decision(&counter, &kept_names, args.threshold),
        );
        println!();
        println!(
            "  {} at {}",
            r["id"].as_str().unwrap_or("?"),
            r["at"].as_str().unwrap_or("?")
        );
        println!("    {:<28} recorded  baseline  counterfactual", "arrow");
        for (i, n) in names.iter().enumerate() {
            let rec = recorded[i].map_or("  —  ".into(), |x| format!("{x:.2}"));
            let cf = c[i].map_or("—".into(), |x| format!("{x:.2}"));
            println!("    {n:<28} {rec:>8}  {:>8.2}  {cf:>14}", b[i]);
        }
        if let Some(a) = with {
            let cf = probs(&counter)
                .get(kept.len())
                .map_or("—".into(), |x| format!("{x:.2}"));
            println!("    {a:<28} {:>8}  {:>8}  {cf:>14}", "  —  ", "—");
        }
        if let (Answer::Choice(x), Answer::Choice(y)) = (&base, &counter) {
            let rec = r["judge"]["none_of_these"]
                .as_f64()
                .map_or("  —  ".into(), |v| format!("{v:.2}"));
            println!(
                "    {:<28} {rec:>8}  {:>8.2}  {:>14.2}",
                "(none of these)", x.none_of_these, y.none_of_these
            );
        }
        let stable = db == r["outcome"]["arrow"].as_str().map(str::to_owned);
        if !stable {
            unstable += 1;
        }
        let verdict = if db != dc {
            changed += 1;
            "DECISION CHANGES"
        } else {
            "same decision"
        };
        println!(
            "    confidence: baseline {} · counterfactual {}",
            conf(&base),
            conf(&counter)
        );
        println!(
            "    decision: baseline {} · counterfactual {} → {verdict}{}",
            db.as_deref().unwrap_or("escalate"),
            dc.as_deref().unwrap_or("escalate"),
            if stable {
                ""
            } else {
                " (the baseline already differs from the record: the model is unstable here)"
            }
        );
    }
    println!();
    println!(
        "{changed} of {} decision(s) change under the counterfactual; {unstable} baseline(s) differ from the record",
        chosen.len()
    );
    Ok(())
}

/// The question the record's model was asked, rebuilt from the category:
/// the candidates it judged (those with a judgment), in record order.
fn request(
    cat: &Category,
    r: &Value,
    state: Value,
) -> Result<(FrameRequest, Vec<String>), BoxError> {
    let at = cat.object_id(r["at"].as_str().unwrap_or_default())?;
    let object = cat.object(at);
    let mut candidates = Vec::new();
    for c in r["candidates"].as_array().into_iter().flatten() {
        if c["judgment"].is_null() {
            continue;
        }
        let a = cat.arrow_id(c["arrow"].as_str().unwrap_or_default())?;
        candidates.push(Candidate::of(cat, a));
    }
    if object.frame.primitive == Primitive::Score {
        candidates.sort_by_key(|c| c.level);
    }
    let names = candidates.iter().map(|c| c.arrow.clone()).collect();
    Ok((
        FrameRequest {
            state,
            at: object.name.clone(),
            focus: None,
            primitive: object.frame.primitive,
            instructions: object.frame.instructions.clone(),
            candidates,
            can_fork: false,
            parallel: object.frame.parallel,
        },
        names,
    ))
}

fn probs(a: &Answer) -> Vec<f32> {
    match a {
        Answer::Choice(d) => d.arrows.clone(),
        Answer::Noul { holds, .. } => holds.clone(),
        Answer::Score { levels, .. } => levels.clone(),
    }
}

/// The arrow the answer points to (choice top, most likely level, most
/// likely holding noul arrow), or `None` for an escalation: none of these,
/// or confidence below the walk's gate (as in `walk::decide`).
fn decision(a: &Answer, names: &[String], threshold: f32) -> Option<String> {
    let i = match a {
        Answer::Choice(d) => d
            .top()
            .filter(|(_, p)| d.confidence.unwrap_or(*p) >= threshold)
            .map(|(i, _)| i),
        Answer::Noul { holds, .. } => holds
            .iter()
            .enumerate()
            .filter(|(_, p)| **p >= 0.5)
            .max_by(|x, y| x.1.total_cmp(y.1))
            .map(|(i, _)| i),
        Answer::Score {
            levels, confidence, ..
        } => levels
            .iter()
            .enumerate()
            .max_by(|x, y| x.1.total_cmp(y.1))
            .filter(|(_, p)| confidence.unwrap_or(**p) >= threshold)
            .map(|(i, _)| i),
    }?;
    names.get(i).cloned()
}

fn remove(v: &mut Value, dotted: &str) {
    let (parent, last) = match dotted.rsplit_once('.') {
        Some((p, l)) => (p.split('.').try_fold(&mut *v, |v, k| v.get_mut(k)), l),
        None => (Some(v), dotted),
    };
    if let Some(Value::Object(m)) = parent {
        m.remove(last);
    }
}

fn set(v: &mut Value, dotted: &str, value: Value) {
    let mut cursor = v;
    let parts: Vec<&str> = dotted.split('.').collect();
    for (i, p) in parts.iter().enumerate() {
        let Some(m) = cursor.as_object_mut() else {
            return;
        };
        if i + 1 == parts.len() {
            m.insert((*p).to_owned(), value);
            return;
        }
        cursor = m
            .entry((*p).to_owned())
            .or_insert_with(|| Value::Object(serde_json::Map::new()));
    }
}

fn conf(a: &Answer) -> String {
    a.confidence().map_or("—".into(), |c| format!("{c:.2}"))
}
