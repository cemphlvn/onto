//! The trace projection: telemetry events as typed raster events.
//!
//! A renderer (the ECharts page of `onto raster`, a figure exporter, a
//! live view) consumes [`Raster`]; nothing downstream reads telemetry.
//! Every event belongs to a frame record (a visit, or a join), and every
//! record knows its causal parents: the walk's previous record, the fork a
//! branch came from, the records a join merged.

use std::collections::{BTreeMap, HashMap};

use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    /// A walk's stay at a frame (interval).
    Visit,
    /// Waiting for the frame claim (interval).
    ClaimWait,
    /// A System-1 call (interval).
    JudgeCall,
    /// A System-2 call (interval).
    ProposerCall,
    /// Waiting at a join for siblings (interval).
    JoinWait,
    /// Arriving at a frame (point); opacity = the confidence of the
    /// decision that led here, unless the step was mechanical.
    Arrival,
    Fork,
    Join,
    /// The winner of a race join.
    RaceWin,
    /// An intersection with another walk (frame claims).
    Potentiality,
    /// Structure proposed or transported at an escalation.
    Proposal,
    Admitted,
    Held,
    Refused,
    Escalation,
    Failed,
}

#[derive(Clone, Debug, Serialize)]
pub struct RasterEvent {
    pub id: usize,
    /// The frame record the event belongs to.
    pub record: Option<String>,
    pub category: String,
    pub frame: String,
    pub walk: u64,
    pub kind: EventKind,
    pub start_ns: u64,
    pub end_ns: Option<u64>,
    pub confidence: Option<f32>,
    /// A mechanical step (split frame): no judgment, full opacity.
    pub mechanical: bool,
    /// Short human text for tooltips.
    pub label: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Walk {
    pub id: u64,
    pub parent: Option<u64>,
    /// The case's root walk.
    pub root: u64,
    pub case: Option<String>,
    pub goal: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Raster {
    pub category: String,
    /// Row order: declared order when known, learned frames after the
    /// frame they were reached from, else first appearance.
    pub frames: Vec<String>,
    pub walks: Vec<Walk>,
    pub events: Vec<RasterEvent>,
    /// Frame record id → causal parent record ids.
    pub parents: BTreeMap<String, Vec<String>>,
    pub meta: Value,
}

/// Projects one run's telemetry events (as written, with `timestamp`
/// already turned into `t` = milliseconds since the run started).
pub fn project(events: &[Value], declared: &[String]) -> Raster {
    let ns = |e: &Value| (e["t"].as_f64().unwrap_or(0.0).max(0.0) * 1e6) as u64;
    let s = |e: &Value, k: &str| e[k].as_str().unwrap_or_default().to_owned();
    let start = events.iter().find(|e| e["event"] == "run.start");
    let end = events.iter().find(|e| e["event"] == "run.end");
    let category = start.map_or_else(String::new, |e| s(e, "category"));

    // Walks.
    let mut walks: Vec<Walk> = Vec::new();
    for e in events.iter().filter(|e| e["event"] == "walk.start") {
        walks.push(Walk {
            id: e["walk"].as_u64().unwrap_or(0),
            parent: e["parent"].as_u64(),
            root: 0,
            case: e["case"].as_str().map(str::to_owned),
            goal: s(e, "goal"),
        });
    }
    let parent_of: HashMap<u64, Option<u64>> = walks.iter().map(|w| (w.id, w.parent)).collect();
    for w in &mut walks {
        let mut r = w.id;
        while let Some(Some(p)) = parent_of.get(&r) {
            r = *p;
        }
        w.root = r;
    }
    let walk_end: HashMap<u64, u64> = events
        .iter()
        .filter(|e| {
            matches!(
                e["event"].as_str(),
                Some("walk.end" | "walk.refused" | "walk.failed")
            )
        })
        .filter_map(|e| Some((e["walk"].as_u64()?, ns(e))))
        .collect();

    // Visits, with their bounds: next visit or join of the walk, or its end.
    struct V {
        walk: u64,
        at: String,
        record: String,
        start: u64,
        claimed: u64,
        end: u64,
    }
    let mut by_walk: BTreeMap<u64, Vec<&Value>> = BTreeMap::new();
    for e in events {
        if let Some(w) = e["walk"].as_u64() {
            by_walk.entry(w).or_default().push(e);
        }
    }
    let mut visits: Vec<V> = Vec::new();
    for (w, list) in &by_walk {
        let bounds: Vec<(u64, &Value)> = list
            .iter()
            .filter_map(|e| {
                let t = ns(e);
                match e["event"].as_str()? {
                    "visit" => Some((
                        t.saturating_sub((e["claim_wait_ms"].as_f64().unwrap_or(0.0) * 1e6) as u64),
                        *e,
                    )),
                    "join" => Some((
                        t.saturating_sub((e["wait_ms"].as_f64().unwrap_or(0.0) * 1e6) as u64),
                        *e,
                    )),
                    _ => None,
                }
            })
            .collect();
        for (i, (st, e)) in bounds.iter().enumerate() {
            if e["event"] != "visit" {
                continue;
            }
            let claimed = ns(e);
            let next = bounds[i + 1..].iter().map(|b| b.0).find(|t| *t >= claimed);
            let end = next
                .or_else(|| walk_end.get(w).copied())
                .unwrap_or(claimed)
                .max(claimed);
            visits.push(V {
                walk: *w,
                at: s(e, "at"),
                record: s(e, "record"),
                start: *st,
                claimed,
                end,
            });
        }
    }
    let containing = |walk: u64, t: u64, at: Option<&str>| -> Option<String> {
        visits
            .iter()
            .rfind(|v| {
                v.walk == walk
                    && at.is_none_or(|a| v.at == a)
                    && v.start <= t + 1_000_000
                    && t <= v.end + 1_000_000
            })
            .map(|v| v.record.clone())
    };

    let mut out: Vec<RasterEvent> = Vec::new();
    let mut push = |record: Option<String>,
                    frame: String,
                    walk: u64,
                    kind: EventKind,
                    start: u64,
                    end: Option<u64>,
                    confidence: Option<f32>,
                    mechanical: bool,
                    label: String| {
        let id = out.len();
        out.push(RasterEvent {
            id,
            record,
            category: category.clone(),
            frame,
            walk,
            kind,
            start_ns: start,
            end_ns: end,
            confidence,
            mechanical,
            label,
        });
    };

    // Intervals and arrivals from visits.
    for v in &visits {
        push(
            Some(v.record.clone()),
            v.at.clone(),
            v.walk,
            EventKind::Visit,
            v.start,
            Some(v.end),
            None,
            false,
            format!("at {} for {}", v.at, dur(v.end - v.start)),
        );
        if v.claimed > v.start + 500_000 {
            push(
                Some(v.record.clone()),
                v.at.clone(),
                v.walk,
                EventKind::ClaimWait,
                v.start,
                Some(v.claimed),
                None,
                false,
                format!("waited {} for the claim", dur(v.claimed - v.start)),
            );
        }
        // The decision that led here: the walk's last step before arriving.
        let step = by_walk.get(&v.walk).and_then(|l| {
            l.iter().rfind(|e| {
                e["event"] == "step" && ns(e) <= v.start + 1_000_000 && e["to"] == v.at.as_str()
            })
        });
        let (conf, mechanical, label) = match step {
            Some(e) => {
                let mech = e["decided_by"] == "split";
                let c = e["confidence"]
                    .as_f64()
                    .or(e["p"].as_f64())
                    .map(|x| x as f32);
                let how = if mech {
                    "a required step (no judgment)".to_owned()
                } else {
                    format!("{} {:.2}", s(e, "decided_by"), c.unwrap_or(1.0))
                };
                (
                    c,
                    mech,
                    format!("arrived at {} via {} ({how})", v.at, s(e, "arrow")),
                )
            }
            None => (None, false, format!("arrived at {}", v.at)),
        };
        push(
            Some(v.record.clone()),
            v.at.clone(),
            v.walk,
            EventKind::Arrival,
            v.start,
            None,
            if mechanical { None } else { conf },
            mechanical,
            label,
        );
    }

    // Everything else.
    for e in events {
        let Some(walk) = e["walk"].as_u64() else {
            continue;
        };
        let t = ns(e);
        let at = s(e, "at");
        let ms_of = |k: &str| (e[k].as_f64().unwrap_or(0.0) * 1e6) as u64;
        match e["event"].as_str().unwrap_or_default() {
            "judge.call" => {
                let st = t.saturating_sub(ms_of("latency_ms"));
                let conf = e["confidence"].as_f64().map(|x| x as f32);
                let label = format!(
                    "judge at {at}: {} question(s), {}{}",
                    e["questions"],
                    dur(t - st),
                    conf.map_or(String::new(), |c| format!(", confidence {c:.2}"))
                );
                push(
                    containing(walk, t, Some(&at)),
                    at,
                    walk,
                    EventKind::JudgeCall,
                    st,
                    Some(t),
                    conf,
                    false,
                    label,
                );
            }
            "proposer.call" => {
                let st = t.saturating_sub(ms_of("latency_ms"));
                let label = format!(
                    "proposer at {at}: {} proposal(s), {}",
                    e["proposals"],
                    dur(t - st)
                );
                push(
                    containing(walk, t, Some(&at)),
                    at,
                    walk,
                    EventKind::ProposerCall,
                    st,
                    Some(t),
                    None,
                    false,
                    label,
                );
            }
            "join" => {
                let st = t.saturating_sub(ms_of("wait_ms"));
                let record = e["record"].as_str().map(str::to_owned);
                if t > st + 500_000 {
                    push(
                        record.clone(),
                        at.clone(),
                        walk,
                        EventKind::JoinWait,
                        st,
                        Some(t),
                        None,
                        false,
                        format!("waited {} at the {} join", dur(t - st), s(e, "policy")),
                    );
                }
                let race_win = e["policy"] == "race" && e["role"] == "continued";
                let kind = if race_win {
                    EventKind::RaceWin
                } else {
                    EventKind::Join
                };
                push(
                    record,
                    at,
                    walk,
                    kind,
                    t,
                    None,
                    None,
                    false,
                    format!(
                        "{} join, {}: {}",
                        s(e, "policy"),
                        s(e, "role"),
                        s(e, "detail")
                    ),
                );
            }
            "fork" => push(
                e["record"].as_str().map(str::to_owned),
                at,
                walk,
                EventKind::Fork,
                t,
                None,
                None,
                false,
                format!("fork into {}", s(e, "branches")),
            ),
            "escalate" => push(
                containing(walk, t, Some(&at)),
                at,
                walk,
                EventKind::Escalation,
                t,
                None,
                None,
                false,
                format!("escalated: {}", s(e, "reason")),
            ),
            "expansion" => {
                let record = e["record"].as_str().map(str::to_owned);
                let n = |k: &str| e[k].as_u64().unwrap_or(0);
                push(
                    record.clone(),
                    at.clone(),
                    walk,
                    EventKind::Proposal,
                    t,
                    None,
                    None,
                    false,
                    format!("{} at {at}", s(e, "source")),
                );
                for (k, kind, word) in [
                    ("learned", EventKind::Admitted, "learned"),
                    ("held", EventKind::Held, "held for a person"),
                    ("refused", EventKind::Refused, "refused"),
                ] {
                    if n(k) > 0 {
                        push(
                            record.clone(),
                            at.clone(),
                            walk,
                            kind,
                            t,
                            None,
                            None,
                            false,
                            format!("{} {word}", n(k)),
                        );
                    }
                }
            }
            "potentiality" => {
                let with = e["with"]
                    .as_u64()
                    .map_or(String::new(), |w| format!(" with walk {w}"));
                push(
                    containing(walk, t, Some(&at)),
                    at,
                    walk,
                    EventKind::Potentiality,
                    t,
                    None,
                    None,
                    false,
                    format!(
                        "{} {}{with}: {}",
                        s(e, "kind"),
                        s(e, "resolution"),
                        s(e, "nodes")
                    ),
                );
            }
            "walk.refused" | "walk.failed" => push(
                None,
                at,
                walk,
                EventKind::Failed,
                t,
                None,
                None,
                false,
                s(e, "error"),
            ),
            _ => {}
        }
    }

    // Causal parents per record.
    let mut parents: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut last_record: HashMap<u64, String> = HashMap::new();
    let mut records_in_order: Vec<(u64, u64, String)> = visits
        .iter()
        .map(|v| (v.start, v.walk, v.record.clone()))
        .collect();
    for e in events.iter().filter(|e| e["event"] == "join") {
        if let (Some(w), Some(r)) = (e["walk"].as_u64(), e["record"].as_str()) {
            records_in_order.push((
                ns(e).saturating_sub((e["wait_ms"].as_f64().unwrap_or(0.0) * 1e6) as u64),
                w,
                r.to_owned(),
            ));
            let merged: Vec<String> = e["merged"]
                .as_str()
                .unwrap_or_default()
                .split(',')
                .filter(|x| !x.is_empty())
                .map(str::to_owned)
                .collect();
            parents.entry(r.to_owned()).or_default().extend(merged);
        }
    }
    records_in_order.sort();
    for (_, w, r) in &records_in_order {
        let entry = parents.entry(r.clone()).or_default();
        match last_record.get(w) {
            Some(prev) => entry.insert(0, prev.clone()),
            None => {
                // A branch's first record follows its parent's fork.
                if let Some(Some(p)) = parent_of.get(w)
                    && let Some(f) = events
                        .iter()
                        .filter(|e| e["event"] == "fork" && e["walk"].as_u64() == Some(*p))
                        .filter(|e| {
                            e["spawned"]
                                .as_str()
                                .unwrap_or_default()
                                .split(',')
                                .any(|x| x == w.to_string())
                        })
                        .find_map(|e| e["record"].as_str())
                {
                    entry.insert(0, f.to_owned());
                }
            }
        }
        last_record.insert(*w, r.clone());
    }
    for v in parents.values_mut() {
        v.dedup();
    }

    // Rows.
    let mut seen: Vec<String> = Vec::new();
    for e in &out {
        if !e.frame.is_empty() && !seen.contains(&e.frame) {
            seen.push(e.frame.clone());
        }
    }
    let frames = if declared.is_empty() {
        seen
    } else {
        let mut rows: Vec<String> = declared
            .iter()
            .filter(|d| seen.contains(d))
            .cloned()
            .collect();
        for name in seen.iter().filter(|n| !declared.contains(n)) {
            let from = events
                .iter()
                .find(|e| e["event"] == "step" && e["to"] == name.as_str())
                .and_then(|e| e["from"].as_str());
            let at = from.and_then(|f| rows.iter().position(|r| r == f));
            match at {
                Some(i) => {
                    // After the source row and any learned rows already there.
                    let mut j = i + 1;
                    while j < rows.len() && !declared.contains(&rows[j]) {
                        j += 1;
                    }
                    rows.insert(j, name.clone());
                }
                None => rows.push(name.clone()),
            }
        }
        rows
    };

    let meta = serde_json::json!({
        "category": category,
        "judge": start.map(|e| s(e, "judge")),
        "proposer": start.map(|e| s(e, "proposer")),
        "policy": start.map(|e| s(e, "policy")),
        "open_world": start.and_then(|e| e["open_world"].as_bool()),
        "wall_ms": end.and_then(|e| e["wall_ms"].as_f64()),
        "model_ms_sum": end.and_then(|e| e["model_ms_sum"].as_f64()),
        "judge_calls": end.and_then(|e| e["judge_calls"].as_u64()),
        "proposer_calls": end.and_then(|e| e["proposer_calls"].as_u64()),
        "learned": if declared.is_empty() {
            Vec::new()
        } else {
            frames.iter().filter(|f| !declared.contains(f)).cloned().collect::<Vec<_>>()
        },
    });
    Raster {
        category,
        frames,
        walks,
        events: out,
        parents,
        meta,
    }
}

fn dur(ns: u64) -> String {
    let ms = ns as f64 / 1e6;
    if ms >= 1000.0 {
        format!("{:.2} s", ms / 1000.0)
    } else {
        format!("{ms:.0} ms")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn projects_visits_calls_arrivals_and_parents() {
        let ev = vec![
            json!({"event": "run.start", "t": 0.0, "category": "C"}),
            json!({"event": "walk.start", "t": 1.0, "walk": 1, "case": "K-1", "goal": "g"}),
            json!({"event": "visit", "t": 5.0, "walk": 1, "at": "A", "record": "w1.1", "claim_wait_ms": 2.0}),
            json!({"event": "judge.call", "t": 15.0, "walk": 1, "at": "A", "latency_ms": 8.0, "questions": 1}),
            json!({"event": "step", "t": 16.0, "walk": 1, "from": "A", "arrow": "go", "to": "B", "decided_by": "choice", "p": 0.8, "confidence": 0.9}),
            json!({"event": "visit", "t": 17.0, "walk": 1, "at": "B", "record": "w1.2", "claim_wait_ms": 0.0}),
            json!({"event": "escalate", "t": 20.0, "walk": 1, "at": "B", "reason": "open_frame"}),
            json!({"event": "walk.end", "t": 21.0, "walk": 1}),
        ];
        let r = project(&ev, &["B".into(), "A".into()]);
        assert_eq!(r.frames, ["B", "A"], "declared order");
        let of = |k: EventKind| r.events.iter().filter(move |e| e.kind == k);
        let visit_a = of(EventKind::Visit).find(|e| e.frame == "A").unwrap();
        assert_eq!(
            (visit_a.start_ns, visit_a.end_ns),
            (3_000_000, Some(17_000_000))
        );
        assert_eq!(of(EventKind::ClaimWait).count(), 1);
        let judge = of(EventKind::JudgeCall).next().unwrap();
        assert_eq!(judge.record.as_deref(), Some("w1.1"));
        assert_eq!(
            (judge.start_ns, judge.end_ns),
            (7_000_000, Some(15_000_000))
        );
        let arrival_b = of(EventKind::Arrival).find(|e| e.frame == "B").unwrap();
        assert_eq!(arrival_b.confidence, Some(0.9));
        assert_eq!(r.parents["w1.2"], ["w1.1"]);
        assert_eq!(
            of(EventKind::Escalation).next().unwrap().record.as_deref(),
            Some("w1.2")
        );
        assert_eq!(r.walks[0].case.as_deref(), Some("K-1"));
    }
}
