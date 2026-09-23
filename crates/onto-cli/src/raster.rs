//! `onto raster`: what a run did, in time (`docs/06-spaces.md` §5).
//!
//! Reads a telemetry file (`onto run --telemetry`), keeps one run, projects
//! it to typed raster events (`onto_runtime::trace`), and writes one
//! self-contained HTML page: the projection as data, and the ECharts
//! renderer built from `web/raster` (`assets/raster.js`, Apache-2.0).
//! `--dispositions` embeds the frame records, so clicking an event shows
//! the visit's record (tokens, evidence, candidates, what the model saw).
//! `--category` orders the rows as the category declares its objects.

use std::path::PathBuf;

use clap::Args;
use serde_json::{Map, Value, json};

use crate::run::BoxError;

const TEMPLATE: &str = include_str!("../assets/raster.html");
/// Built by `npm run build` in `web/raster`.
const RENDERER: &str = include_str!("../assets/raster.js");

#[derive(Args)]
pub struct RasterArgs {
    /// A telemetry file (`onto run --telemetry`).
    telemetry: PathBuf,
    /// Frame records of the same run (`onto run --dispositions`).
    #[arg(long)]
    dispositions: Option<PathBuf>,
    /// The category (`FILE` or `FILE#Name`), to order rows as declared.
    #[arg(long)]
    category: Option<PathBuf>,
    /// Which run in the file (1 = first; default: the last).
    #[arg(long)]
    run: Option<usize>,
    /// Where to write the page (default: next to the telemetry, `.html`).
    #[arg(long)]
    out: Option<PathBuf>,
}

pub fn main(args: RasterArgs) -> Result<(), BoxError> {
    let text = std::fs::read_to_string(&args.telemetry)
        .map_err(|e| format!("{}: {e}", args.telemetry.display()))?;
    let events: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    // Split into runs at each `run.start`.
    let mut runs: Vec<Vec<Value>> = Vec::new();
    for e in events {
        if e["event"] == "run.start" || runs.is_empty() {
            runs.push(Vec::new());
        }
        runs.last_mut().expect("pushed").push(e);
    }
    let n = args.run.unwrap_or(runs.len());
    let run = runs
        .get(n.wrapping_sub(1))
        .ok_or_else(|| format!("the file holds {} run(s); --run {n} is not one", runs.len()))?;
    let t0 = run
        .first()
        .and_then(|e| epoch_ms(e["timestamp"].as_str()?))
        .ok_or("no timestamps in the telemetry")?;
    let kept: Vec<Value> = run
        .iter()
        .filter(|e| e["event"] != "mem.sample")
        .filter_map(|e| {
            let t = epoch_ms(e["timestamp"].as_str()?)? - t0;
            let mut m: Map<String, Value> = e.as_object()?.clone();
            m.remove("timestamp");
            m.remove("level");
            m.insert("t".into(), json!((t * 1000.0).round() / 1000.0));
            Some(Value::Object(m))
        })
        .collect();

    let mut records = Map::new();
    if let Some(path) = &args.dispositions {
        for l in std::fs::read_to_string(path)?.lines() {
            if let Ok(r) = serde_json::from_str::<Value>(l)
                && let Some(id) = r["id"].as_str()
            {
                records.insert(id.to_owned(), r);
            }
        }
    }
    let rows: Vec<String> = match &args.category {
        Some(spec) => crate::module::load_category(spec)?
            .objects()
            .iter()
            .map(|o| o.name.clone())
            .collect(),
        None => Vec::new(),
    };
    let raster = onto_runtime::trace::project(&kept, &rows);
    let events = raster.events.len();
    let data = json!({
        "source": args.telemetry.display().to_string(),
        "run": n,
        "runs": runs.len(),
        "raster": raster,
        "records": records,
    });
    // `</` would end an embedding script early.
    let data = serde_json::to_string(&data)?.replace("</", "<\\/");
    let page = TEMPLATE
        .replace("/*DATA*/null", &data)
        .replace("/*RASTER_JS*/", &RENDERER.replace("</script", "<\\/script"));
    let out = args
        .out
        .clone()
        .unwrap_or_else(|| args.telemetry.with_extension("raster.html"));
    std::fs::write(&out, page)?;
    println!(
        "raster: {} ({events} raster events, run {n} of {}{})",
        out.display(),
        runs.len(),
        if records.is_empty() {
            String::new()
        } else {
            format!(", {} frame records", records.len())
        }
    );
    Ok(())
}

/// Milliseconds since the Unix epoch for an RFC 3339 UTC timestamp
/// (`2026-09-23T23:11:35.112518Z`), as telemetry writes them.
fn epoch_ms(ts: &str) -> Option<f64> {
    let (date, time) = ts.trim_end_matches('Z').split_once('T')?;
    let mut d = date.split('-').map(|x| x.parse::<i64>());
    let (y, m, day) = (d.next()?.ok()?, d.next()?.ok()?, d.next()?.ok()?);
    let mut t = time.split(':');
    let (h, min) = (
        t.next()?.parse::<f64>().ok()?,
        t.next()?.parse::<f64>().ok()?,
    );
    let sec = t.next()?.parse::<f64>().ok()?;
    // Days from civil (Howard Hinnant's algorithm).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(((days as f64 * 24.0 + h) * 60.0 + min) * 60_000.0 + sec * 1000.0)
}

#[cfg(test)]
mod tests {
    #[test]
    fn epoch() {
        assert_eq!(super::epoch_ms("1970-01-01T00:00:01.5Z"), Some(1500.0));
        let a = super::epoch_ms("2026-09-23T23:59:59.9Z").unwrap();
        let b = super::epoch_ms("2026-09-24T00:00:00.1Z").unwrap();
        assert!((b - a - 200.0).abs() < 1e-6);
    }
}
