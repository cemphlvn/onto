//! `onto why`: read a disposition file back as explanations.
//!
//! Works on the JSON form of `FrameRecord`s, so the same printer serves
//! files written by `onto run --dispositions` and live records in `onto ask`.

use std::path::PathBuf;

use clap::Args;
use serde_json::Value;

use crate::run::BoxError;

#[derive(Args)]
pub struct WhyArgs {
    /// A JSON-lines file written by `onto run --dispositions`.
    file: PathBuf,
    /// Only this walk (and the visits it follows from).
    #[arg(long)]
    walk: Option<u64>,
}

pub fn main(args: WhyArgs) -> Result<(), BoxError> {
    let text =
        std::fs::read_to_string(&args.file).map_err(|e| format!("{}: {e}", args.file.display()))?;
    let records: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    let shown: Vec<&Value> = match args.walk {
        None => records.iter().collect(),
        Some(w) => lineage(&records, w),
    };
    for r in shown {
        print_record(r, "");
        println!();
    }
    Ok(())
}

/// The walk's records plus the chain of visits it follows from (for a
/// branch, its parent's visits up to the fork).
fn lineage(records: &[Value], walk: u64) -> Vec<&Value> {
    let by_id = |id: &str| records.iter().find(|r| r["id"] == id);
    let own: Vec<&Value> = records.iter().filter(|r| r["walk"] == walk).collect();
    let mut chain = Vec::new();
    let mut cursor = own
        .first()
        .and_then(|r| r["after"].as_str())
        .and_then(by_id);
    while let Some(r) = cursor {
        chain.push(r);
        cursor = r["after"].as_str().and_then(by_id);
    }
    chain.reverse();
    chain.extend(own);
    chain
}

pub fn print_record(r: &Value, pad: &str) {
    let f = |v: &Value| v.as_f64().map_or("  — ".to_owned(), |x| format!("{x:.2}"));
    let after = r["after"]
        .as_str()
        .map_or(String::new(), |a| format!("  after {a}"));
    println!(
        "{pad}{}  at {}  [{}, {}]{after}",
        r["id"].as_str().unwrap_or("?"),
        r["at"].as_str().unwrap_or("?"),
        r["primitive"].as_str().unwrap_or("?"),
        r["closure"].as_str().unwrap_or("?"),
    );
    let j = &r["judge"];
    if j.is_object() {
        let mut parts = vec![format!(
            "{} · {} question(s) · {:.0}ms",
            j["model"].as_str().unwrap_or("?"),
            j["questions"],
            j["latency_ms"].as_f64().unwrap_or(0.0)
        )];
        if !j["confidence"].is_null() {
            parts.push(format!("confidence {}", f(&j["confidence"])));
        }
        if !j["none_of_these"].is_null() {
            parts.push(format!("none_of_these {}", f(&j["none_of_these"])));
        }
        if !j["fork_p"].is_null() {
            parts.push(format!("fork {}", f(&j["fork_p"])));
        }
        println!("{pad}  judge: {}", parts.join(" · "));
    } else {
        println!("{pad}  judge: not asked");
    }
    let wait = r["claim"]["wait_ms"].as_f64().unwrap_or(0.0);
    if wait >= 1.0 {
        println!("{pad}  waited {wait:.0}ms for an intersecting frame");
    }
    for c in r["candidates"].as_array().into_iter().flatten() {
        let kind = c["disposition"]["kind"].as_str().unwrap_or("?");
        let mark = match kind {
            "selected" => "✓",
            "forked" => "⑂",
            "alternative" => "~",
            "rejected" => "·",
            "deferred" => "…",
            "filtered_by_require" => "⊘",
            "blocked_by_entry" => "⊗",
            _ => "?",
        };
        let branch = c["disposition"]["branch"]
            .as_u64()
            .map_or(String::new(), |b| format!(" → walk {b}"));
        let target = format!(
            "{} → {}",
            c["arrow"].as_str().unwrap_or("?"),
            c["to"].as_str().unwrap_or("?")
        );
        println!(
            "{pad}  {mark} {target:<34} {}  {kind}{branch}: {}",
            f(&c["judgment"]),
            c["reason"].as_str().unwrap_or("")
        );
    }
    let o = &r["outcome"];
    let outcome = match o["kind"].as_str() {
        Some("followed") => format!(
            "followed {} → {}",
            o["arrow"].as_str().unwrap_or("?"),
            o["to"].as_str().unwrap_or("?")
        ),
        Some("forked") => format!(
            "forked: continued along {}, spawned {}",
            o["continued"].as_str().unwrap_or("?"),
            o["spawned"]
        ),
        Some("escalated") => format!("escalated ({})", o["reason"].as_str().unwrap_or("?")),
        Some("failed") => format!("failed: {}", o["error"].as_str().unwrap_or("?")),
        Some("joined") => format!(
            "join ({}) {}: {}{}",
            o["policy"].as_str().unwrap_or("?"),
            o["role"].as_str().unwrap_or("?"),
            o["detail"].as_str().unwrap_or(""),
            r["merged_from"]
                .as_array()
                .filter(|m| !m.is_empty())
                .map_or(String::new(), |m| format!(
                    "  [merged from {}]",
                    m.iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
        ),
        _ => "?".into(),
    };
    println!("{pad}  outcome: {outcome}");
    for p in r["proposals"].as_array().into_iter().flatten() {
        println!(
            "{pad}  provisional {}: {} -> {}  ({})",
            p["arrow"].as_str().unwrap_or("?"),
            p["src"].as_str().unwrap_or("?"),
            p["dst"].as_str().unwrap_or("?"),
            p["about"].as_str().unwrap_or("")
        );
    }
}
