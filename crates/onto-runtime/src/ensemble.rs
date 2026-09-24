//! Running an ensemble: one engine per column, the same cases, one shared
//! category (`docs/05-functors.md` §6).
//!
//! Columns walk independently (their judges never see each other). Every
//! arrival is mapped through the column's functor into the shared
//! category; code compares the columns' positions as they move and writes
//! when they first agree and when they first contradict. At the end each
//! case gets an outcome under the ensemble's consensus policy; a surprise
//! is routed by the shared category's admission: a person (assured,
//! sealed) or a gap signal for curation (open world).

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use onto_core::ensemble::{Consensus, Ensemble, compatible};
use onto_core::{Admission, Category, Functor, ObjId};
use serde::Serialize;
use serde_json::json;

use crate::curation::GapSignal;
use crate::engine::{Engine, Job, PositionEvent, RunReport};
use crate::model::{Critic, Judge, Proposer};

pub struct ColumnRun<J, P> {
    pub name: String,
    pub engine: Arc<Engine<J, P>>,
    pub functor: Functor,
    pub start: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ColumnPosition {
    pub column: String,
    pub object: String,
    pub shared: Option<String>,
    pub at_ms: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct CaseOutcome {
    pub job: usize,
    pub case: Option<String>,
    pub goal: String,
    /// Each column's last position that maps into the shared category.
    pub positions: Vec<ColumnPosition>,
    /// `agreed`, `surprise`, `undecided` (the columns agree only where
    /// they started: none concluded anything) or `incomplete` (a column
    /// never reached the functor's domain).
    pub status: String,
    pub agreed: Option<String>,
    pub dissent: Vec<String>,
    /// Pairs of columns whose final positions contradict.
    pub contradictions: Vec<(String, String)>,
    pub first_confirm_ms: Option<f64>,
    pub first_surprise_ms: Option<f64>,
    /// For a surprise: `person` or `curation`.
    pub route: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct EnsembleReport {
    pub ensemble: String,
    pub shared: String,
    pub consensus: String,
    pub columns: Vec<String>,
    pub cases: Vec<CaseOutcome>,
    /// One run report per column, in column order.
    pub runs: Vec<RunReport>,
    /// Contradictions at open-world shared frames, for curation.
    pub gaps: Vec<GapSignal>,
    pub wall_ms: f64,
}

#[derive(Default)]
struct Track {
    /// Per column: (shared position, column object, ms).
    last: BTreeMap<usize, (ObjId, String, f64)>,
    confirmed: Option<f64>,
    surprised: Option<f64>,
}

pub async fn run<J, P>(
    e: &Ensemble,
    shared: Arc<Category>,
    columns: Vec<ColumnRun<J, P>>,
    jobs: Vec<Job>,
) -> Result<EnsembleReport, onto_core::Error>
where
    J: Judge + Critic,
    P: Proposer,
{
    let t0 = Instant::now();
    tracing::info!(
        target: "onto",
        event = "ensemble.start",
        ensemble = %e.name,
        shared = %e.shared,
        columns = %columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(","),
        cases = jobs.len(),
    );
    let tracks: Arc<Mutex<BTreeMap<usize, Track>>> = Arc::default();
    let n = columns.len();
    // Where each column starts, in the shared category: a column still
    // there has not concluded anything.
    let starts: Vec<Option<ObjId>> = columns
        .iter()
        .map(|c| {
            c.engine
                .cat()
                .object_id(&c.start)
                .ok()
                .and_then(|x| c.functor.object(x))
        })
        .collect();
    let mut listeners = Vec::new();
    for (k, col) in columns.iter().enumerate() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<PositionEvent>();
        col.engine.observe(tx);
        let (tracks, shared, functor) = (tracks.clone(), shared.clone(), col.functor.clone());
        let (engine, name, ens) = (col.engine.clone(), col.name.clone(), e.name.clone());
        let starts = starts.clone();
        listeners.push(tokio::spawn(async move {
            while let Some(ev) = rx.recv().await {
                // Learned objects are outside the functor: no position.
                let Ok(x) = engine.cat().object_id(&ev.object) else { continue };
                let Some(y) = functor.object(x) else { continue };
                let ms = ev.at.duration_since(t0).as_secs_f64() * 1000.0;
                let mut all = tracks.lock().unwrap();
                let t = all.entry(ev.job).or_default();
                t.last.insert(k, (y, ev.object.clone(), ms));
                tracing::info!(
                    target: "onto",
                    event = "ensemble.position",
                    ensemble = %ens,
                    job = ev.job,
                    column = %name,
                    walk = ev.walk,
                    object = %ev.object,
                    shared = %shared.object(y).name,
                );
                let pos: Vec<(usize, ObjId)> = t.last.iter().map(|(c, v)| (*c, v.0)).collect();
                let clash = pos.iter().enumerate().find_map(|(i, a)| {
                    pos[i + 1..]
                        .iter()
                        .find(|b| !compatible(&shared, a.1, b.1))
                        .map(|b| (a.0, b.0))
                });
                match clash {
                    Some((a, b)) if t.surprised.is_none() => {
                        t.surprised = Some(ms);
                        tracing::info!(
                            target: "onto",
                            event = "ensemble.surprise",
                            ensemble = %ens,
                            job = ev.job,
                            walk = ev.walk,
                            columns = %format!("{a},{b}"),
                            positions = %format!("{} | {}", shared.object(t.last[&a].0).name, shared.object(t.last[&b].0).name),
                            shared = %shared.object(y).name,
                        );
                    }
                    // Confirmed once every column has concluded something
                    // (left its start) and all still agree.
                    None if pos.len() == n
                        && pos.iter().all(|(c, y)| starts[*c] != Some(*y))
                        && t.confirmed.is_none() =>
                    {
                        t.confirmed = Some(ms);
                        tracing::info!(
                            target: "onto",
                            event = "ensemble.confirm",
                            ensemble = %ens,
                            job = ev.job,
                            walk = ev.walk,
                            shared = %shared.object(y).name,
                        );
                    }
                    _ => {}
                }
            }
        }));
    }

    let runs = futures_join(
        columns
            .iter()
            .map(|c| {
                let jobs: Vec<Job> = jobs
                    .iter()
                    .map(|j| Job {
                        from: c.start.clone(),
                        ..j.clone()
                    })
                    .collect();
                let engine = c.engine.clone();
                async move { engine.run(jobs).await }
            })
            .collect(),
    )
    .await?;
    for c in &columns {
        c.engine.unobserve();
    }
    for l in listeners {
        let _ = l.await;
    }

    let tracks = Arc::try_unwrap(tracks)
        .ok()
        .map(|m| m.into_inner().unwrap())
        .unwrap_or_default();
    let names: Vec<String> = columns.iter().map(|c| c.name.clone()).collect();
    let mut cases = Vec::new();
    let mut gaps = Vec::new();
    for (job, j) in jobs.iter().enumerate() {
        let t = tracks.get(&job);
        let last = t.map(|t| &t.last);
        let positions: Vec<ColumnPosition> = names
            .iter()
            .enumerate()
            .filter_map(|(k, name)| {
                let (y, obj, ms) = last?.get(&k)?;
                Some(ColumnPosition {
                    column: name.clone(),
                    object: obj.clone(),
                    shared: Some(shared.object(*y).name.clone()),
                    at_ms: *ms,
                })
            })
            .collect();
        let ids: Vec<(usize, ObjId)> = last
            .map(|l| l.iter().map(|(k, v)| (*k, v.0)).collect())
            .unwrap_or_default();
        let contradictions: Vec<(String, String)> = ids
            .iter()
            .enumerate()
            .flat_map(|(i, a)| {
                ids[i + 1..]
                    .iter()
                    .filter(|b| !compatible(&shared, a.1, b.1))
                    .map(|b| (names[a.0].clone(), names[b.0].clone()))
                    .collect::<Vec<_>>()
            })
            .collect();
        // The largest pairwise-compatible set of columns.
        let best = largest_compatible(&shared, &ids);
        let needed = match e.consensus {
            Consensus::All => n,
            Consensus::Quorum(q) => q,
        };
        let (status, agreed, dissent) = if ids.len() < needed && contradictions.is_empty() {
            ("incomplete", None, Vec::new())
        } else if best.len() >= needed {
            let furthest = furthest(
                &shared,
                &best
                    .iter()
                    .map(|k| ids.iter().find(|x| x.0 == *k).unwrap().1)
                    .collect::<Vec<_>>(),
            );
            let dissent = names
                .iter()
                .enumerate()
                .filter(|(k, _)| !best.contains(k))
                .map(|(_, n)| n.clone())
                .collect();
            if best
                .iter()
                .all(|k| furthest.is_some() && starts[*k] == furthest)
            {
                ("undecided", None, Vec::new())
            } else {
                (
                    "agreed",
                    furthest.map(|y| shared.object(y).name.clone()),
                    dissent,
                )
            }
        } else {
            ("surprise", None, Vec::new())
        };
        let route = (status == "surprise").then(|| {
            let guarded = ids
                .iter()
                .any(|(_, y)| shared.admission(*y) != Admission::OpenWorld);
            if guarded { "person" } else { "curation" }.to_owned()
        });
        if route.as_deref() == Some("curation") {
            // The frame is where the perspectives went different ways: the
            // furthest object that still reaches every position. The key
            // names the conflicting positions, so the same blind spot in
            // many cases is one gap.
            let ys: Vec<ObjId> = ids.iter().map(|x| x.1).collect();
            let at = divergence(&shared, &ys);
            let side = |c: &String| {
                positions
                    .iter()
                    .find(|p| &p.column == c)
                    .and_then(|p| p.shared.clone())
            };
            let mut sides: Vec<String> = contradictions
                .iter()
                .flat_map(|(a, b)| [side(a), side(b)])
                .flatten()
                .collect();
            sides.sort();
            sides.dedup();
            let frame = at.map_or(e.shared.clone(), |z| shared.object(z).name.clone());
            let key = format!(
                "{}:{frame}:contradiction:{}",
                shared.snapshot().map_or("-", |s| &s[..s.len().min(8)]),
                sides.join("|")
            );
            let options = at.map_or_else(Vec::new, |z| {
                shared
                    .out(z)
                    .iter()
                    .map(|a| {
                        let x = shared.arrow(*a);
                        format!("{} → {}", x.name, shared.object(x.dst).name)
                    })
                    .collect()
            });
            gaps.push(GapSignal {
                key,
                kind: "contradiction".into(),
                frame,
                reason: format!("perspectives of ensemble {} disagree", e.name),
                missing: positions
                    .iter()
                    .map(|p| format!("{} sees {}", p.column, p.shared.clone().unwrap_or_default()))
                    .collect::<Vec<_>>()
                    .join("; "),
                record: format!("{}#{job}", e.name),
                walk: 0,
                case: j.case["id"].as_str().map(str::to_owned),
                snapshot: shared.snapshot().map(str::to_owned),
                seen: json!({ "positions": positions }),
                options,
            });
        }
        let outcome = CaseOutcome {
            job,
            case: j.case["id"].as_str().map(str::to_owned),
            goal: j.goal.clone(),
            positions,
            status: status.into(),
            agreed,
            dissent,
            contradictions,
            first_confirm_ms: t.and_then(|t| t.confirmed),
            first_surprise_ms: t.and_then(|t| t.surprised),
            route,
        };
        tracing::info!(
            target: "onto",
            event = "ensemble.outcome",
            ensemble = %e.name,
            job,
            case = outcome.case.as_deref(),
            status = %outcome.status,
            agreed = outcome.agreed.as_deref(),
            route = outcome.route.as_deref(),
        );
        cases.push(outcome);
    }
    Ok(EnsembleReport {
        ensemble: e.name.clone(),
        shared: e.shared.clone(),
        consensus: match e.consensus {
            Consensus::All => "all".into(),
            Consensus::Quorum(q) => format!("quorum {q}"),
        },
        columns: names,
        cases,
        runs,
        gaps,
        wall_ms: t0.elapsed().as_secs_f64() * 1000.0,
    })
}

async fn futures_join<F>(futs: Vec<F>) -> Result<Vec<RunReport>, onto_core::Error>
where
    F: std::future::Future<Output = Result<RunReport, onto_core::Error>>,
{
    let mut out = Vec::with_capacity(futs.len());
    let handles: Vec<_> = futs.into_iter().map(Box::pin).collect();
    for r in join_all(handles).await {
        out.push(r?);
    }
    Ok(out)
}

/// Polls every future to completion concurrently.
async fn join_all<F: std::future::Future + Unpin>(mut futs: Vec<F>) -> Vec<F::Output> {
    let mut out: Vec<Option<F::Output>> = futs.iter().map(|_| None).collect();
    std::future::poll_fn(|cx| {
        let mut pending = false;
        for (i, f) in futs.iter_mut().enumerate() {
            if out[i].is_none() {
                match std::pin::Pin::new(f).poll(cx) {
                    std::task::Poll::Ready(v) => out[i] = Some(v),
                    std::task::Poll::Pending => pending = true,
                }
            }
        }
        if pending {
            std::task::Poll::Pending
        } else {
            std::task::Poll::Ready(())
        }
    })
    .await;
    out.into_iter().map(|v| v.expect("ready")).collect()
}

/// Columns forming the largest pairwise-compatible set (columns are few).
fn largest_compatible(shared: &Category, ids: &[(usize, ObjId)]) -> Vec<usize> {
    let n = ids.len();
    let mut best: Vec<usize> = Vec::new();
    for mask in 1u32..(1 << n) {
        let set: Vec<&(usize, ObjId)> = (0..n)
            .filter(|i| mask & (1 << i) != 0)
            .map(|i| &ids[i])
            .collect();
        let ok = set
            .iter()
            .enumerate()
            .all(|(i, a)| set[i + 1..].iter().all(|b| compatible(shared, a.1, b.1)));
        if ok && set.len() > best.len() {
            best = set.iter().map(|x| x.0).collect();
        }
    }
    best
}

/// Where positions went different ways: the furthest object that reaches
/// (or is) every one of them.
fn divergence(shared: &Category, ys: &[ObjId]) -> Option<ObjId> {
    let reaches = |z: ObjId, y: ObjId| z == y || shared.reachable(z, y, &[]).is_some();
    let common: Vec<ObjId> = (0..shared.objects().len() as u32)
        .map(ObjId)
        .filter(|z| ys.iter().all(|y| reaches(*z, *y)))
        .collect();
    common
        .iter()
        .copied()
        .find(|z| common.iter().all(|w| reaches(*w, *z)))
}

/// The position every other one reaches (the furthest along).
fn furthest(shared: &Category, ys: &[ObjId]) -> Option<ObjId> {
    ys.iter().copied().find(|p| {
        ys.iter()
            .all(|q| q == p || shared.reachable(*q, *p, &[]).is_some())
    })
}
