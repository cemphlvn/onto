//! The learned-structure library: dormant arrows are out of the graph
//! (no presence in judgments) and answer a structure gap at their frame
//! before a catalogue or a model; retired arrows are never recalled.

use std::sync::Arc;
use std::time::Duration;

use onto_core::walk::Proposal;
use onto_runtime::engine::{ArrowState, Learned, StepRecord};
use onto_runtime::model::{MockJudge, MockProposer};
use onto_runtime::{Config, Engine, Job, RunReport};
use serde_json::json;

const TRIAGE: &str = include_str!("../../../examples/triage.onto");

fn refund(state: ArrowState) -> Learned {
    Learned {
        proposal: Proposal {
            arrow: "refund".into(),
            src: "Request".into(),
            dst: "Refund".into(),
            about: "the customer asks for money back".into(),
            ..Default::default()
        },
        record: "w1.1".into(),
        reason: "low_confidence".into(),
        snapshot: None,
        checks: vec!["well_formed: pass".into()],
        transported: None,
        state,
        note: None,
    }
}

async fn run(library: Vec<Learned>, goals: &[&str]) -> RunReport {
    let engine = Engine::new(
        Arc::new(onto_core::parse(TRIAGE).unwrap()),
        MockJudge {
            latency: Duration::from_millis(5),
        },
        MockProposer {
            latency: Duration::from_millis(5),
        },
        Config::default(),
    );
    engine.use_library(library);
    let jobs = goals
        .iter()
        .map(|g| Job {
            from: "Request".into(),
            goal: (*g).into(),
            case: json!({}),
        })
        .collect();
    engine.run(jobs).await.unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_dormant_arrow_answers_the_gap_before_the_proposer() {
    let r = run(vec![refund(ArrowState::Dormant)], &["please refund me"]).await;
    // The gap at Request cost no proposer call. (Refund, a leaf, is an
    // open frame: the open world goes on from there.)
    let proposed_at_request = r.walks[0].steps.iter().any(|s| {
        matches!(s, StepRecord::Expanded { at, source, .. } if at == "Request" && source != "library")
    });
    assert!(!proposed_at_request);
    assert_eq!(r.recalled, vec![("refund".to_owned(), "w1.1".to_owned())]);
    match &r.walks[0].steps[0] {
        StepRecord::Expanded {
            source, learned, ..
        } => {
            assert_eq!(source, "library");
            assert_eq!(learned[0].arrow, "refund");
        }
        s => panic!("expected a recall, got {s:?}"),
    }
    // Re-judged with the recalled arrow present: the walk takes it.
    assert!(r.walks[0].path.contains("refund"), "{}", r.walks[0].path);
    // A recall is not learning: the layer gets no new entry for it.
    assert!(r.learned.iter().all(|l| l.proposal.arrow != "refund"));
}

#[tokio::test(flavor = "multi_thread")]
async fn dormant_arrows_have_no_presence_until_recalled() {
    // A case with no gap never sees the dormant arrow among its candidates.
    let r = run(
        vec![refund(ArrowState::Dormant)],
        &["a bug: patch it and ship"],
    )
    .await;
    assert!(r.recalled.is_empty());
    let first = &r.walks[0].frames[0];
    assert!(first.candidates.iter().all(|c| c.arrow != "refund"));
}

#[tokio::test(flavor = "multi_thread")]
async fn retired_and_empty_libraries_leave_the_gap_to_the_proposer() {
    for library in [vec![refund(ArrowState::Retired)], Vec::new()] {
        let r = run(library, &["please refund me"]).await;
        assert!(r.recalled.is_empty());
        assert!(r.proposer_calls >= 1);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn walks_judged_before_a_recall_judge_again_instead_of_proposing() {
    // Four refund requests judged at once: one recalls, the others find
    // the frame grown when they reach their gap and judge again.
    let goals = ["please refund me"; 4];
    let r = run(vec![refund(ArrowState::Dormant)], &goals).await;
    assert_eq!(r.recalled.len(), 1);
    for w in r.walks.iter().filter(|w| w.parent.is_none()) {
        assert!(w.path.contains("refund"), "{}", w.path);
        let proposed = w.steps.iter().any(|s| {
            matches!(s, StepRecord::Expanded { at, source, .. } if at == "Request" && source == "proposer")
        });
        assert!(!proposed, "walk {} asked the proposer at Request", w.walk);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn structure_that_hung_from_a_dormant_arrow_comes_back_with_it() {
    let pay_back = Learned {
        proposal: Proposal {
            arrow: "pay_back".into(),
            src: "Refund".into(),
            dst: "Done".into(),
            ..Default::default()
        },
        ..refund(ArrowState::Active)
    };
    let r = run(
        vec![refund(ArrowState::Dormant), pay_back],
        &["please refund me, then done"],
    )
    .await;
    assert_eq!(r.walks[0].path, "pay_back.refund : Request -> Done");
    assert_eq!(r.proposer_calls, 0);
}
