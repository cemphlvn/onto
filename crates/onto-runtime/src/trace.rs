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
    /// What the raster shows, measured (`insights`).
    pub insights: Insights,
}

/// Measurements of a run's behaviour as a system, computed from the
/// projection: one per question the raster answers.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Insights {
    pub wall_ms: f64,
    /// Where the time went, summed over all walks.
    pub split: Split,
    /// Walks active at once.
    pub concurrency: Concurrency,
    /// Per frame, busiest first.
    pub frames: Vec<FrameStat>,
    /// The chain of records that ended last, back to the run's start.
    pub critical_path: CriticalPath,
    /// Every join that resolved, with each branch's arrival.
    pub joins: Vec<JoinStat>,
    /// Every learned arrow: the frame before and after it became active.
    pub learning: Vec<LearnStat>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Split {
    pub judge_ms: f64,
    pub proposer_ms: f64,
    pub claim_wait_ms: f64,
    pub join_wait_ms: f64,
    /// Time in visits not spent on the above (runtime work, idle).
    pub other_ms: f64,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Concurrency {
    pub max: usize,
    /// Time-weighted mean of walks active, over the run.
    pub mean: f64,
    /// `(ms, walks active from then on)`, a step function.
    pub profile: Vec<(f64, usize)>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct FrameStat {
    pub frame: String,
    pub visits: usize,
    pub cases: usize,
    pub time_ms: f64,
    pub claim_wait_ms: f64,
    /// Most walks waiting for this frame's claim at once.
    pub max_queue: usize,
    pub model_ms: f64,
    pub escalations: usize,
    /// Distinct cases that escalated here.
    pub escalating_cases: usize,
    pub proposals: usize,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct CriticalPath {
    pub records: Vec<String>,
    pub frames: Vec<String>,
    pub end_ms: f64,
    pub split: Split,
    /// Sum of all visit time over the wall time: work done in parallel.
    pub work_over_wall: f64,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct JoinStat {
    pub record: String,
    pub frame: String,
    pub policy: String,
    pub continued: u64,
    /// `(walk, ms it reached the join)`, in arrival order.
    pub arrivals: Vec<(u64, f64)>,
    /// The branch that arrived last (all), first (race), or never came
    /// (an incomplete join).
    pub decisive: u64,
    pub spread_ms: f64,
    /// `continued`, or `incomplete` when a sibling ended without arriving.
    pub outcome: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct LearnStat {
    pub arrow: String,
    pub frame: String,
    pub at_ms: f64,
    pub source: String,
    pub visits_before: usize,
    pub escalations_before: usize,
    pub mean_stay_before_ms: f64,
    pub visits_after: usize,
    pub escalations_after: usize,
    pub mean_stay_after_ms: f64,
    /// Steps along the learned arrow after it became active.
    pub used_after: usize,
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
            // The next visit or join of this walk ends this visit (its
            // recorded start can round a hair before this claim).
            let next = bounds.get(i + 1).map(|b| b.0);
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
                let rec = containing(walk, t, Some(&at));
                let label = format!(
                    "proposer at {at}: {} proposal(s), {}",
                    e["proposals"],
                    dur(t - st)
                );
                push(
                    rec.clone(),
                    at.clone(),
                    walk,
                    EventKind::ProposerCall,
                    st,
                    Some(t),
                    None,
                    false,
                    label,
                );
                // Every proposal round is a mark: a row of them is a gap.
                push(
                    rec,
                    at.clone(),
                    walk,
                    EventKind::Proposal,
                    t,
                    None,
                    None,
                    false,
                    format!("proposer: {} proposal(s) at {at}", e["proposals"]),
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
                // Proposer rounds are marked at their call; transport here.
                if e["source"] == "transport" {
                    push(
                        record.clone(),
                        at.clone(),
                        walk,
                        EventKind::Proposal,
                        t,
                        None,
                        None,
                        false,
                        format!("transport at {at}"),
                    );
                }
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
    let mut raster = Raster {
        category,
        frames,
        walks,
        events: out,
        parents,
        meta,
        insights: Insights::default(),
    };
    raster.insights = insights(&raster, events);
    raster
}

fn insights(r: &Raster, telemetry: &[Value]) -> Insights {
    let m = |ns: u64| ns as f64 / 1e6;
    let span = |e: &RasterEvent| m(e.end_ns.unwrap_or(e.start_ns).saturating_sub(e.start_ns));
    let of = |k: EventKind| r.events.iter().filter(move |e| e.kind == k);
    let wall = r
        .meta
        .get("wall_ms")
        .and_then(Value::as_f64)
        .unwrap_or_else(|| {
            r.events
                .iter()
                .map(|e| m(e.end_ns.unwrap_or(e.start_ns)))
                .fold(0.0, f64::max)
        });
    let root: HashMap<u64, u64> = r.walks.iter().map(|w| (w.id, w.root)).collect();

    let sum = |k: EventKind| of(k).map(span).sum::<f64>();
    let visit_time: f64 = sum(EventKind::Visit);
    let split = Split {
        judge_ms: sum(EventKind::JudgeCall),
        proposer_ms: sum(EventKind::ProposerCall),
        claim_wait_ms: sum(EventKind::ClaimWait),
        join_wait_ms: sum(EventKind::JoinWait),
        other_ms: 0.0,
    };
    let split = Split {
        other_ms: (visit_time + split.join_wait_ms
            - split.judge_ms
            - split.proposer_ms
            - split.claim_wait_ms
            - split.join_wait_ms)
            .max(0.0),
        ..split
    };

    // Concurrency: walks with a visit or a join wait open at time t.
    let mut edges: Vec<(u64, i64)> = Vec::new();
    for e in r
        .events
        .iter()
        .filter(|e| matches!(e.kind, EventKind::Visit | EventKind::JoinWait))
    {
        edges.push((e.start_ns, 1));
        edges.push((e.end_ns.unwrap_or(e.start_ns), -1));
    }
    edges.sort_by_key(|x| (x.0, x.1));
    let (mut active, mut max, mut area, mut last) = (0i64, 0usize, 0.0, 0u64);
    let mut profile = Vec::new();
    for (t, d) in edges {
        area += active as f64 * m(t - last);
        last = t;
        active += d;
        max = max.max(active.max(0) as usize);
        if profile
            .last()
            .is_none_or(|p: &(f64, usize)| p.1 != active.max(0) as usize)
        {
            profile.push((m(t), active.max(0) as usize));
        }
    }
    let concurrency = Concurrency {
        max,
        mean: if wall > 0.0 { area / wall } else { 0.0 },
        profile,
    };

    // Frames.
    let mut frames: Vec<FrameStat> = r
        .frames
        .iter()
        .map(|f| {
            let here = |k: EventKind| {
                r.events
                    .iter()
                    .filter(move |e| e.kind == k && &e.frame == f)
            };
            let waits: Vec<&RasterEvent> = here(EventKind::ClaimWait).collect();
            let mut qe: Vec<(u64, i64)> = waits
                .iter()
                .flat_map(|e| [(e.start_ns, 1), (e.end_ns.unwrap_or(e.start_ns), -1)])
                .collect();
            qe.sort_by_key(|x| (x.0, x.1));
            let (mut q, mut qmax) = (0i64, 0i64);
            for (_, d) in qe {
                q += d;
                qmax = qmax.max(q);
            }
            let esc: Vec<&RasterEvent> = here(EventKind::Escalation).collect();
            let mut cases: Vec<u64> = here(EventKind::Visit).map(|e| root[&e.walk]).collect();
            cases.sort_unstable();
            cases.dedup();
            let mut esc_cases: Vec<u64> = esc.iter().map(|e| root[&e.walk]).collect();
            esc_cases.sort_unstable();
            esc_cases.dedup();
            FrameStat {
                frame: f.clone(),
                visits: here(EventKind::Visit).count(),
                cases: cases.len(),
                time_ms: here(EventKind::Visit).map(span).sum(),
                claim_wait_ms: waits.iter().map(|e| span(e)).sum(),
                max_queue: qmax.max(0) as usize,
                model_ms: here(EventKind::JudgeCall)
                    .chain(here(EventKind::ProposerCall))
                    .map(span)
                    .sum(),
                escalations: esc.len(),
                escalating_cases: esc_cases.len(),
                proposals: here(EventKind::Proposal).count(),
            }
        })
        .collect();
    frames.sort_by(|a, b| b.time_ms.total_cmp(&a.time_ms));

    // Critical path: from the record that ended last, back through the
    // parent that ended last (the one the next record waited for).
    let mut rec_start: HashMap<&str, u64> = HashMap::new();
    let mut rec_end: HashMap<&str, u64> = HashMap::new();
    let mut rec_frame: HashMap<&str, &str> = HashMap::new();
    for e in &r.events {
        if let Some(rec) = e.record.as_deref() {
            let s = rec_start.entry(rec).or_insert(e.start_ns);
            *s = (*s).min(e.start_ns);
            let t = rec_end.entry(rec).or_insert(0);
            *t = (*t).max(e.end_ns.unwrap_or(e.start_ns));
            rec_frame.entry(rec).or_insert(e.frame.as_str());
        }
    }
    let mut path: Vec<String> = Vec::new();
    let mut cur = rec_end.iter().max_by_key(|x| *x.1).map(|x| x.0.to_string());
    while let Some(rec) = cur {
        if path.contains(&rec) {
            break;
        }
        cur = r
            .parents
            .get(&rec)
            .and_then(|ps| {
                ps.iter()
                    .filter(|p| rec_end.contains_key(p.as_str()))
                    .max_by_key(|p| rec_end[p.as_str()])
            })
            .cloned();
        path.push(rec);
    }
    path.reverse();
    let on = |e: &&RasterEvent| {
        e.record
            .as_deref()
            .is_some_and(|x| path.iter().any(|p| p == x))
    };
    let path_sum = |k: EventKind| {
        r.events
            .iter()
            .filter(on)
            .filter(|e| e.kind == k)
            .map(span)
            .sum::<f64>()
    };
    let path_visits = path_sum(EventKind::Visit);
    let cp_split = Split {
        judge_ms: path_sum(EventKind::JudgeCall),
        proposer_ms: path_sum(EventKind::ProposerCall),
        claim_wait_ms: path_sum(EventKind::ClaimWait),
        join_wait_ms: path_sum(EventKind::JoinWait),
        other_ms: 0.0,
    };
    let critical_path = CriticalPath {
        frames: path
            .iter()
            .map(|p| {
                rec_frame
                    .get(p.as_str())
                    .copied()
                    .unwrap_or_default()
                    .to_owned()
            })
            .collect(),
        end_ms: path
            .last()
            .and_then(|p| rec_end.get(p.as_str()))
            .map_or(0.0, |t| m(*t)),
        split: Split {
            other_ms: (path_visits + cp_split.join_wait_ms
                - cp_split.judge_ms
                - cp_split.proposer_ms
                - cp_split.claim_wait_ms
                - cp_split.join_wait_ms)
                .max(0.0),
            ..cp_split
        },
        work_over_wall: if wall > 0.0 { visit_time / wall } else { 0.0 },
        records: path,
    };

    // Joins: the continuing walk, and every walk folded into it there.
    let ms_f = |e: &Value, k: &str| e[k].as_f64().unwrap_or(0.0);
    let mut joins = Vec::new();
    for c in telemetry
        .iter()
        .filter(|e| e["event"] == "join" && e["role"] == "continued")
    {
        let (Some(walk), Some(at)) = (c["walk"].as_u64(), c["at"].as_str()) else {
            continue;
        };
        let mut arrivals = vec![(walk, ms_f(c, "t") - ms_f(c, "wait_ms"))];
        for o in telemetry
            .iter()
            .filter(|e| e["event"] == "join" && e["at"] == at && e["into"].as_u64() == Some(walk))
        {
            if let Some(w) = o["walk"].as_u64() {
                arrivals.push((w, ms_f(o, "t") - ms_f(o, "wait_ms")));
            }
        }
        arrivals.sort_by(|a, b| a.1.total_cmp(&b.1));
        let policy = c["policy"].as_str().unwrap_or_default().to_owned();
        let decisive = if policy == "race" {
            arrivals.first().map_or(walk, |a| a.0)
        } else {
            arrivals.last().map_or(walk, |a| a.0)
        };
        let spread = arrivals.last().map_or(0.0, |l| l.1) - arrivals.first().map_or(0.0, |f| f.1);
        joins.push(JoinStat {
            record: c["record"].as_str().unwrap_or_default().to_owned(),
            frame: at.to_owned(),
            policy,
            continued: walk,
            arrivals,
            decisive,
            spread_ms: spread,
            outcome: "continued".into(),
        });
    }
    // Incomplete joins: the waiting walks, and the sibling that never came.
    let mut seen_incomplete: Vec<(String, u64)> = Vec::new();
    for c in telemetry
        .iter()
        .filter(|e| e["event"] == "join" && e["role"] == "escalated")
    {
        let detail = c["detail"].as_str().unwrap_or_default();
        let Some(missing) = detail
            .split("walk(s) ")
            .nth(1)
            .and_then(|x| x.split(|ch: char| !ch.is_ascii_digit()).next())
            .and_then(|x| x.parse::<u64>().ok())
        else {
            continue;
        };
        let at = c["at"].as_str().unwrap_or_default().to_owned();
        if seen_incomplete.contains(&(at.clone(), missing)) {
            continue;
        }
        seen_incomplete.push((at.clone(), missing));
        let mut arrivals: Vec<(u64, f64)> = telemetry
            .iter()
            .filter(|e| {
                e["event"] == "join"
                    && e["role"] == "escalated"
                    && e["at"] == at.as_str()
                    && e["detail"] == detail
            })
            .filter_map(|e| Some((e["walk"].as_u64()?, ms_f(e, "t") - ms_f(e, "wait_ms"))))
            .collect();
        arrivals.sort_by(|a, b| a.1.total_cmp(&b.1));
        let first = arrivals.first().map_or(0.0, |a| a.1);
        joins.push(JoinStat {
            record: c["record"].as_str().unwrap_or_default().to_owned(),
            frame: at,
            policy: c["policy"].as_str().unwrap_or_default().to_owned(),
            continued: 0,
            arrivals,
            decisive: missing,
            spread_ms: ms_f(c, "t") - first,
            outcome: "incomplete".into(),
        });
    }

    // Learning: each learned arrow, its frame before and after.
    let mut learning = Vec::new();
    for l in telemetry.iter().filter(|e| e["event"] == "learned") {
        let (Some(arrow), Some(frame)) = (l["arrow"].as_str(), l["from"].as_str()) else {
            continue;
        };
        let t = (ms_f(l, "t") * 1e6) as u64;
        let source = telemetry
            .iter()
            .filter(|e| {
                e["event"] == "expansion"
                    && e["record"] == l["record"]
                    && e["learned"].as_u64().unwrap_or(0) > 0
            })
            .find_map(|e| e["source"].as_str())
            .unwrap_or("proposer")
            .to_owned();
        let visits: Vec<&RasterEvent> = of(EventKind::Visit).filter(|e| e.frame == frame).collect();
        let (before, after): (Vec<&RasterEvent>, Vec<&RasterEvent>) =
            visits.iter().partition(|e| e.start_ns < t);
        let esc = |b: bool| {
            of(EventKind::Escalation)
                .filter(|e| e.frame == frame && (e.start_ns < t) == b)
                .count()
        };
        let mean = |v: &[&RasterEvent]| {
            if v.is_empty() {
                0.0
            } else {
                v.iter().map(|e| span(e)).sum::<f64>() / v.len() as f64
            }
        };
        learning.push(LearnStat {
            arrow: arrow.to_owned(),
            frame: frame.to_owned(),
            at_ms: m(t),
            source,
            visits_before: before.len(),
            escalations_before: esc(true),
            mean_stay_before_ms: mean(&before),
            visits_after: after.len(),
            escalations_after: esc(false),
            mean_stay_after_ms: mean(&after),
            used_after: telemetry
                .iter()
                .filter(|e| {
                    e["event"] == "step" && e["arrow"] == arrow && ms_f(e, "t") * 1e6 > t as f64
                })
                .count(),
        });
    }

    Insights {
        wall_ms: wall,
        split,
        concurrency,
        frames,
        critical_path,
        joins,
        learning,
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

    #[test]
    fn insights_blame_the_last_branch_and_count_before_after_learning() {
        let v = |t: f64, w: u64, at: &str, rec: &str| json!({"event": "visit", "t": t, "walk": w, "at": at, "record": rec, "claim_wait_ms": 0.0});
        let ev = vec![
            json!({"event": "run.start", "t": 0.0, "category": "C"}),
            json!({"event": "walk.start", "t": 0.0, "walk": 1, "goal": "g"}),
            json!({"event": "walk.start", "t": 1.0, "walk": 2, "parent": 1, "goal": "g"}),
            v(0.0, 1, "A", "w1.1"),
            json!({"event": "fork", "t": 1.0, "walk": 1, "at": "A", "record": "w1.1", "spawned": "2"}),
            v(1.0, 2, "B", "w2.1"),
            v(1.0, 1, "C", "w1.2"),
            // walk 1 reaches the join at 3, waits until 9; walk 2 arrives at 9.
            json!({"event": "join", "t": 9.0, "walk": 1, "at": "J", "record": "w1.3", "merged": "w2.1", "policy": "all", "role": "continued", "wait_ms": 6.0}),
            json!({"event": "join", "t": 9.0, "walk": 2, "at": "J", "record": "w2.2", "policy": "all", "role": "ended", "into": 1, "wait_ms": 0.0}),
            // learning at K: an escalation before, a use after.
            v(10.0, 1, "K", "w1.4"),
            json!({"event": "escalate", "t": 11.0, "walk": 1, "at": "K", "reason": "none_of_these"}),
            json!({"event": "learned", "t": 12.0, "record": "w1.4", "arrow": "new", "from": "K", "to": "N"}),
            json!({"event": "step", "t": 13.0, "walk": 1, "from": "K", "arrow": "new", "to": "N", "decided_by": "choice", "p": 1.0}),
            v(14.0, 1, "K", "w1.5"),
            json!({"event": "walk.end", "t": 15.0, "walk": 1}),
            json!({"event": "walk.end", "t": 9.0, "walk": 2}),
        ];
        let r = project(&ev, &[]);
        let j = &r.insights.joins[0];
        assert_eq!(
            (j.frame.as_str(), j.decisive),
            ("J", 2),
            "the join waited for walk 2"
        );
        assert!((j.spread_ms - 6.0).abs() < 1e-6);
        let l = &r.insights.learning[0];
        assert_eq!(
            (l.escalations_before, l.escalations_after, l.used_after),
            (1, 0, 1)
        );
        assert_eq!((l.visits_before, l.visits_after), (1, 1));
        assert!(r.insights.concurrency.max >= 2);
        assert_eq!(r.parents["w2.1"], ["w1.1"], "a branch follows its fork");
    }
}
