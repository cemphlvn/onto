use std::sync::Arc;
use std::time::Duration;

use onto_runtime::engine::StepRecord;
use onto_runtime::frames::{PotentialityKind, Resolution};
use onto_runtime::model::{MockChooser, MockProposer};
use onto_runtime::{Config, Engine, Job, Policy, RunReport};

const TRIAGE: &str = include_str!("../../../examples/triage.onto");
const LATENCY: Duration = Duration::from_millis(60);

async fn run(policy: Policy, speculate: bool, goals: &[&str]) -> RunReport {
    let cat = Arc::new(onto_core::parse(TRIAGE).unwrap());
    let cfg = Config {
        policy,
        speculate,
        ..Config::default()
    };
    let engine = Engine::new(
        cat,
        MockChooser { latency: LATENCY },
        MockProposer { latency: LATENCY },
        cfg,
    );
    let jobs = goals
        .iter()
        .map(|g| Job {
            from: "Request".into(),
            goal: (*g).into(),
        })
        .collect();
    engine.run(jobs).await.unwrap()
}

/// The mock chooser follows report -> patch -> ship for this goal: three
/// reads through closed frames, ending at the terminal `Done`.
const FIX: &str = "a bug: patch it and ship";

#[tokio::test(flavor = "multi_thread")]
async fn shared_policy_runs_readers_in_parallel() {
    let r = run(Policy::Shared, false, &[FIX, FIX, FIX]).await;
    for w in &r.walks {
        assert_eq!(w.path, "ship.patch.report : Request -> Done");
    }
    assert!(
        r.potentialities
            .iter()
            .all(|p| p.resolution == Resolution::Coexisted)
    );
    assert!(
        r.potentialities
            .iter()
            .any(|p| p.kind == PotentialityKind::Node)
    );
    // 9 chooser calls of 60ms each; parallel walks finish in about 3 calls' time.
    assert!(
        r.model_ms_sum / r.wall_ms > 2.0,
        "parallelism {:.2}",
        r.model_ms_sum / r.wall_ms
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn exclusive_policy_waits_on_intersecting_frames() {
    let r = run(Policy::Exclusive, false, &[FIX, FIX, FIX]).await;
    for w in &r.walks {
        assert_eq!(w.path, "ship.patch.report : Request -> Done");
    }
    let waited: Vec<_> = r
        .potentialities
        .iter()
        .filter(|p| p.resolution == Resolution::Waited)
        .collect();
    assert!(!waited.is_empty());
    assert!(
        waited
            .iter()
            .all(|p| !p.nodes.is_empty() && p.wait_ms > 0.0)
    );
    assert!(
        r.model_ms_sum / r.wall_ms < 1.5,
        "parallelism {:.2}",
        r.model_ms_sum / r.wall_ms
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn same_new_concept_from_two_walks_is_a_conceptual_potentiality() {
    // No arrow name or target appears in these goals: System 1 declines at
    // Request and both proposers suggest `Refund`.
    let r = run(Policy::Shared, false, &["please refund", "I want a refund"]).await;
    for w in &r.walks {
        assert!(
            matches!(w.steps[0], StepRecord::Escalated { .. }),
            "{:?}",
            w.steps
        );
    }
    let conceptual: Vec<_> = r
        .potentialities
        .iter()
        .filter(|p| p.kind == PotentialityKind::Conceptual)
        .collect();
    assert_eq!(conceptual.len(), 1);
    assert_eq!(conceptual[0].nodes, ["Refund"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn speculation_discards_system2_when_system1_is_confident() {
    let r = run(Policy::Shared, true, &[FIX]).await;
    assert_eq!(r.walks[0].path, "ship.patch.report : Request -> Done");
    assert_eq!(r.speculative_discarded, 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn speculation_uses_system2_answer_on_escalation() {
    let r = run(Policy::Shared, true, &["please refund"]).await;
    let StepRecord::Escalated { proposals, .. } = &r.walks[0].steps[0] else {
        panic!("expected escalation: {:?}", r.walks[0].steps);
    };
    assert_eq!(proposals[0].dst, "Refund");
    assert_eq!(r.speculative_discarded, 0);
    assert_eq!(r.proposer_calls, 1);
}
