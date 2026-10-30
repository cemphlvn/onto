//! `onto calibrate`: fit the calibration map the gate applies
//! (`docs/05-functors.md` §7) on decision records joined with answers.
//!
//! A labelled decision is a record's raw p for the target it chose (or
//! would have chosen, when deferred) and whether a person's answer named
//! that target. Records of several runs, each with its own answers, may
//! be pooled: `--data RECORDS:ANSWERS` repeats.

use std::collections::BTreeMap;
use std::path::PathBuf;

use clap::Args;
use onto_core::calibrate::{self, Labelled, Platt};
use serde_json::{Value, json};

use crate::discover::read_answers;
use crate::run::BoxError;

#[derive(Args)]
pub struct CalibrateArgs {
    /// `RECORDS:ANSWERS`: decision records (`onto discover --records`) and
    /// the answers for the same source objects. Repeat to pool runs.
    #[arg(long, required = true, num_args = 1..)]
    data: Vec<String>,
    /// Weight of the prior that pulls the map towards the identity (in
    /// labels); 0 is plain Platt scaling.
    #[arg(long, default_value_t = 1.0)]
    pull: f64,
    /// Write the map here (JSON), for `onto discover --calibration`.
    #[arg(long)]
    out: Option<PathBuf>,
}

pub fn labelled(records: &PathBuf, answers: &PathBuf) -> Result<Vec<Labelled>, BoxError> {
    let truth: BTreeMap<String, Option<String>> = read_answers(answers)?.into_iter().collect();
    let mut out = Vec::new();
    for line in std::fs::read_to_string(records)?.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let r: Value = serde_json::from_str(line)?;
        let (Some(object), Some(p), Some(target)) =
            (r["object"].as_str(), r["p"].as_f64(), r["target"].as_str())
        else {
            continue;
        };
        if r["disposition"] == "answered" {
            continue;
        }
        if let Some(answer) = truth.get(object) {
            out.push((p, answer.as_deref() == Some(target)));
        }
    }
    Ok(out)
}

pub fn main(args: CalibrateArgs) -> Result<(), BoxError> {
    let mut points = Vec::new();
    for spec in &args.data {
        let (r, a) = spec
            .split_once(':')
            .ok_or_else(|| format!("--data {spec}: expected RECORDS:ANSWERS"))?;
        let got = labelled(&PathBuf::from(r), &PathBuf::from(a))?;
        println!("{r}: {} labelled decisions", got.len());
        points.extend(got);
    }
    if points.is_empty() {
        return Err("no labelled decisions: do the answers name the records' objects?".into());
    }
    let map = Platt::fit(&points, args.pull);
    let mapped: Vec<Labelled> = points.iter().map(|&(p, y)| (map.apply(p), y)).collect();
    let right = points.iter().filter(|p| p.1).count();
    println!(
        "{} decisions, {right} right ({:.3}); platt a {:.3} b {:.3} (pull {})",
        points.len(),
        right as f64 / points.len() as f64,
        map.a,
        map.b,
        args.pull
    );
    println!(
        "  raw      Brier {:.3}  ECE {:.3}",
        calibrate::brier(&points),
        calibrate::ece(&points, 5)
    );
    println!(
        "  mapped   Brier {:.3}  ECE {:.3}   (in-sample: hold a system out to measure)",
        calibrate::brier(&mapped),
        calibrate::ece(&mapped, 5)
    );
    if let Some(path) = &args.out {
        let v = json!({
            "method": "platt", "a": map.a, "b": map.b, "pull": args.pull,
            "points": points.len(), "fit_on": args.data,
        });
        std::fs::write(path, serde_json::to_string_pretty(&v)? + "\n")?;
        println!("written to {}", path.display());
    }
    Ok(())
}
