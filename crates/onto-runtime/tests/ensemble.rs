//! Ensembles (functors phase 3): columns walk the same case independently;
//! code compares their images in the shared category.

use std::sync::Arc;
use std::time::Duration;

use onto_runtime::ensemble::{ColumnRun, EnsembleReport, run};
use onto_runtime::model::{MockJudge, MockProposer};
use onto_runtime::{Config, Engine, Job};
use serde_json::json;

/// Three perspectives on one service. The mock judge follows the words of
/// the goal, so a goal like "aerr blat cslow" sends each column its own
/// way.
fn source(admission: &str, consensus: &str) -> String {
    format!(
        r#"
category Shared {{
    objects: Seen, Fine, Hurt, Code, Load;
    ok: Seen -> Fine; hurt: Seen -> Hurt; code: Hurt -> Code; load: Hurt -> Load;
    admission: {admission};
}}
category A {{
    objects: A0, Aok, Aerr, Ahot;
    start: A0;
    frame A0: choice "what does A show?";
    a_ok: A0 -> Aok; a_err: A0 -> Aerr; a_hot: A0 -> Ahot;
    closed: A0, Aok, Aerr, Ahot;
}}
category B {{
    objects: B0, Bok, Blog, Blat;
    start: B0;
    frame B0: choice "what does B show?";
    b_ok: B0 -> Bok; b_log: B0 -> Blog; b_lat: B0 -> Blat;
    closed: B0, Bok, Blog, Blat;
}}
category C {{
    objects: C0, Cquiet, Cslow;
    start: C0;
    frame C0: choice "what does C show?";
    c_quiet: C0 -> Cquiet; c_slow: C0 -> Cslow;
    closed: C0, Cquiet, Cslow;
}}
functor FA: A -> Shared {{
    objects: A0 -> Seen, Aok -> Fine, Aerr -> Hurt, Ahot -> Load;
    a_ok: ok; a_err: hurt; a_hot: load.hurt;
}}
functor FB: B -> Shared {{
    objects: B0 -> Seen, Bok -> Fine, Blog -> Code, Blat -> Hurt;
    b_ok: ok; b_log: code.hurt; b_lat: hurt;
}}
functor FC: C -> Shared {{
    objects: C0 -> Seen, Cquiet -> Fine, Cslow -> Hurt;
    c_quiet: ok; c_slow: hurt;
}}
ensemble E {{
    shared: Shared;
    column A: FA from A0;
    column B: FB from B0;
    column C: FC from C0;
    consensus: {consensus};
}}
"#
    )
}

async fn ensemble(admission: &str, consensus: &str, goals: &[&str]) -> EnsembleReport {
    let module = onto_core::parse::parse_module(&source(admission, consensus), &mut |p| {
        Err(onto_core::Error::Parse {
            line: 0,
            msg: format!("no import {p}"),
        })
    })
    .unwrap();
    let e = module.ensemble("E").unwrap().clone();
    let shared = Arc::new(module.category("Shared").unwrap().clone());
    let columns = e
        .columns
        .iter()
        .enumerate()
        .map(|(k, c)| {
            let engine = Engine::new(
                Arc::new(module.category(&c.category).unwrap().clone()),
                MockJudge {
                    latency: Duration::from_millis(10 * (k as u64 + 1)),
                },
                MockProposer {
                    latency: Duration::from_millis(10),
                },
                Config {
                    ensemble: Some("E".into()),
                    ..Config::default()
                },
            );
            engine.with_walk_base(1 + k as u64 * 1_000_000);
            ColumnRun {
                name: c.category.clone(),
                engine,
                functor: module.functor(&c.functor).unwrap().clone(),
                start: c.start.clone(),
            }
        })
        .collect();
    let jobs = goals
        .iter()
        .enumerate()
        .map(|(i, g)| Job {
            from: String::new(),
            goal: (*g).into(),
            case: json!({ "id": format!("case-{i}") }),
        })
        .collect();
    run(&e, shared, columns, jobs).await.unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn agreement_is_reachability_and_the_furthest_position_wins() {
    // A sees Hurt, B sees Code (further along), C sees Hurt: compatible.
    let r = ensemble("open_world", "all", &["aerr blog cslow", "aok bok cquiet"]).await;
    let c = &r.cases[0];
    assert_eq!(c.status, "agreed", "{c:?}");
    assert_eq!(c.agreed.as_deref(), Some("Code"));
    assert!(c.first_confirm_ms.is_some() && c.first_surprise_ms.is_none());
    assert_eq!(r.cases[1].agreed.as_deref(), Some("Fine"));
    assert!(r.gaps.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_blind_spot_is_a_surprise_routed_to_curation_in_open_world() {
    // A and B see Fine; C sees Hurt: neither reaches the other.
    let r = ensemble("open_world", "all", &["aok bok cslow"]).await;
    let c = &r.cases[0];
    assert_eq!(c.status, "surprise", "{c:?}");
    assert_eq!(c.contradictions.len(), 2);
    assert_eq!(c.route.as_deref(), Some("curation"));
    assert!(c.first_surprise_ms.is_some());
    let g = &r.gaps[0];
    assert_eq!(g.kind, "contradiction");
    // Where the perspectives went different ways, with its options.
    assert_eq!(g.frame, "Seen");
    assert_eq!(g.options.len(), 2);
    assert!(
        g.key.ends_with(":Seen:contradiction:Fine|Hurt"),
        "{}",
        g.key
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn competing_causes_diverge_below_the_shared_symptom() {
    // A sees Load, B sees Code: both are Hurt, then different causes.
    let r = ensemble("open_world", "all", &["ahot blog cslow"]).await;
    let c = &r.cases[0];
    assert_eq!(c.status, "surprise");
    assert_eq!(c.contradictions, vec![("A".to_owned(), "B".to_owned())]);
    assert_eq!(r.gaps[0].frame, "Hurt");
    assert!(
        r.gaps[0].key.ends_with(":Hurt:contradiction:Code|Load"),
        "{}",
        r.gaps[0].key
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn under_assured_admission_a_surprise_needs_a_person() {
    let r = ensemble("assured", "all", &["aok bok cslow"]).await;
    assert_eq!(r.cases[0].route.as_deref(), Some("person"));
    assert!(r.gaps.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn quorum_reports_dissent_instead_of_surprise() {
    let r = ensemble("open_world", "quorum 2", &["aok bok cslow"]).await;
    let c = &r.cases[0];
    assert_eq!(c.status, "agreed", "{c:?}");
    assert_eq!(c.agreed.as_deref(), Some("Fine"));
    assert_eq!(c.dissent, vec!["C".to_owned()]);
    // The dissent is still a contradiction.
    assert_eq!(c.route.as_deref(), Some("curation"));
    assert_eq!(r.gaps.len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn under_assured_admission_a_quorum_does_not_silence_a_dissent() {
    let r = ensemble("assured", "quorum 2", &["aok bok cslow"]).await;
    let c = &r.cases[0];
    assert_eq!(
        (c.status.as_str(), c.agreed.as_deref()),
        ("agreed", Some("Fine"))
    );
    assert_eq!(c.route.as_deref(), Some("person"));
    assert!(r.gaps.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_column_that_concludes_nothing_is_not_agreement() {
    // C says nothing; A and B agree. Its start position reaches
    // everything, but it does not count.
    let r = ensemble("open_world", "all", &["aerr blog"]).await;
    assert_eq!(r.cases[0].status, "incomplete", "{:?}", r.cases[0]);
    assert_eq!(r.cases[0].route, None);
    let r = ensemble("assured", "all", &["aerr blog"]).await;
    assert_eq!(r.cases[0].route.as_deref(), Some("person"));
    // A quorum of the columns that did conclude is enough.
    let r = ensemble("assured", "quorum 2", &["aerr blog"]).await;
    assert_eq!(
        (r.cases[0].status.as_str(), r.cases[0].agreed.as_deref()),
        ("agreed", Some("Code"))
    );
    assert_eq!(r.cases[0].route, None);
}

#[tokio::test(flavor = "multi_thread")]
async fn columns_that_conclude_nothing_are_undecided() {
    let r = ensemble("open_world", "all", &["nothing to see"]).await;
    assert_eq!(r.cases[0].status, "undecided", "{:?}", r.cases[0]);
    assert!(r.cases[0].first_confirm_ms.is_none());
}
