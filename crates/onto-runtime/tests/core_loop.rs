use std::sync::Arc;
use std::time::Duration;

use onto_runtime::engine::StepRecord;
use onto_runtime::frames::{PotentialityKind, Resolution};
use onto_runtime::model::{MockJudge, MockProposer};
use onto_runtime::{Config, Engine, Job, Policy, RunReport};
use serde_json::{Value, json};

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
        MockJudge { latency: LATENCY },
        MockProposer { latency: LATENCY },
        cfg,
    );
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

/// The mock judge follows report -> patch -> ship for this goal: three
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

const PARTS: &str = r#"
category Parts {
    objects: Alert, Latency, Security, Report, Watch, Page, Consented, Marketing;
    frame Alert: noul;
    latency:  Alert -> Latency  "slow responses";
    security: Alert -> Security "leaked credentials";
    closed: Alert, Latency, Security;

    frame Report: score "How badly are users affected?";
    watch: Report -> Watch level 0 "cosmetic";
    page:  Report -> Page  level 1 "blocking";
    closed: Report, Watch, Page;

    market: Consented -> Marketing "offers" require consent.marketing == true;
    closed: Consented, Marketing;
}
"#;

async fn run_parts(max_branches: usize, from: &str, goal: &str, case: Value) -> RunReport {
    let cat = Arc::new(onto_core::parse(PARTS).unwrap());
    let cfg = Config {
        policy: Policy::Shared,
        max_branches,
        ..Config::default()
    };
    let engine = Engine::new(
        cat,
        MockJudge { latency: LATENCY },
        MockProposer { latency: LATENCY },
        cfg,
    );
    let job = Job {
        from: from.into(),
        goal: goal.into(),
        case,
    };
    engine.run(vec![job]).await.unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn independent_aspects_fork_into_parallel_branches() {
    let r = run_parts(
        4,
        "Alert",
        "checkout is slow and credentials leaked",
        json!({}),
    )
    .await;
    assert_eq!((r.forks, r.branches), (1, 1));
    assert_eq!(r.walks.len(), 2);
    assert_eq!(r.walks[1].parent, Some(r.walks[0].walk));
    let mut ends: Vec<_> = r
        .walks
        .iter()
        .map(|w| w.path.rsplit(" -> ").next().unwrap().to_owned())
        .collect();
    ends.sort();
    assert_eq!(ends, ["Latency", "Security"]);
    assert!(matches!(r.walks[0].steps[0], StepRecord::Forked { .. }));
}

#[tokio::test(flavor = "multi_thread")]
async fn competing_readings_follow_the_best_and_log_the_rest() {
    // No " and ": the mock judges the two as competing readings.
    let r = run_parts(
        4,
        "Alert",
        "slow responses, maybe leaked credentials",
        json!({}),
    )
    .await;
    assert_eq!((r.forks, r.walks.len()), (0, 1));
    let alts: Vec<_> = r
        .potentialities
        .iter()
        .filter(|p| p.kind == PotentialityKind::Alternative)
        .collect();
    assert_eq!(alts.len(), 1);
    let StepRecord::Followed { alternatives, .. } = &r.walks[0].steps[0] else {
        panic!()
    };
    assert_eq!(alternatives.len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn no_branch_budget_means_no_fork_question() {
    let r = run_parts(
        0,
        "Alert",
        "checkout is slow and credentials leaked",
        json!({}),
    )
    .await;
    assert_eq!((r.forks, r.walks.len()), (0, 1));
    // One Noul per arrow, no fork question.
    assert_eq!(r.judge_questions, 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn score_frames_follow_the_level() {
    let r = run_parts(
        4,
        "Report",
        "the checkout is blocking for everyone",
        json!({}),
    )
    .await;
    assert_eq!(r.walks[0].path, "page : Report -> Page");
}

#[tokio::test(flavor = "multi_thread")]
async fn require_blocks_arrows_without_evidence() {
    let goal = "send me offers";
    let yes = run_parts(
        4,
        "Consented",
        goal,
        json!({"consent": {"marketing": true}}),
    )
    .await;
    assert_eq!(yes.walks[0].path, "market : Consented -> Marketing");
    let no = run_parts(4, "Consented", goal, json!({})).await;
    assert!(matches!(
        no.walks[0].steps[0],
        StepRecord::Escalated {
            reason: onto_core::walk::Escalation::NoneOfThese,
            ..
        }
    ));
    assert_eq!(
        no.judge_calls, 0,
        "no model is asked when code already rules the arrow out"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn open_frames_follow_an_arrow_that_fits() {
    // Question is open, but its `answer` arrow fits this goal.
    let r = run(Policy::Shared, false, &["ask a question and get an answer"]).await;
    assert_eq!(r.walks[0].path, "answer.ask : Request -> Done");
    assert_eq!(r.proposer_calls, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn later_proposers_reuse_pending_proposals_at_the_frame() {
    let cat = Arc::new(onto_core::parse(TRIAGE).unwrap());
    let cfg = Config {
        policy: Policy::Shared,
        ..Config::default()
    };
    let engine = Engine::new(
        cat,
        MockJudge { latency: LATENCY },
        MockProposer { latency: LATENCY },
        cfg,
    );
    let job = |goal: &str| Job {
        from: "Request".into(),
        goal: goal.into(),
        case: json!({}),
    };

    engine.run(vec![job("please refund")]).await.unwrap();
    // Without the pending list this would propose `Now`; with it, the
    // proposer sees `Refund` pending at Request and reuses it.
    let r = engine
        .run(vec![job("refund this double charge now")])
        .await
        .unwrap();
    let StepRecord::Escalated { proposals, .. } = &r.walks[0].steps[0] else {
        panic!("{:?}", r.walks[0].steps)
    };
    assert_eq!(
        (proposals[0].arrow.as_str(), proposals[0].dst.as_str()),
        ("to_refund", "Refund")
    );
    assert!(
        r.potentialities
            .iter()
            .any(|p| p.kind == PotentialityKind::Conceptual)
    );
}

mod dispositions {
    use super::*;
    use onto_core::walk::Disposition;

    fn kinds(r: &onto_runtime::record::FrameRecord) -> Vec<(String, Disposition)> {
        r.candidates
            .iter()
            .map(|c| (c.arrow.clone(), c.disposition.clone()))
            .collect()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn every_arrow_at_every_visit_gets_one_disposition() {
        let cat = onto_core::parse(TRIAGE).unwrap();
        let r = run(Policy::Shared, false, &[FIX]).await;
        let frames = &r.walks[0].frames;
        assert_eq!(frames.len(), 3, "Request, Bug, Fix");
        for f in frames {
            let at = cat.object_id(&f.at).unwrap();
            assert_eq!(f.candidates.len(), cat.out(at).len(), "{}", f.at);
            assert_eq!(
                f.candidates
                    .iter()
                    .filter(|c| c.disposition == Disposition::Selected)
                    .count(),
                1
            );
        }
        assert_eq!(frames[0].after, None);
        assert_eq!(frames[1].after.as_deref(), Some(frames[0].id.as_str()));
        let rejected = frames[0]
            .candidates
            .iter()
            .find(|c| c.arrow == "ask")
            .unwrap();
        assert_eq!(rejected.disposition, Disposition::Rejected);
        assert!(
            rejected.reason.contains("below `report`"),
            "{}",
            rejected.reason
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn forks_link_branches_to_their_origin() {
        // Both branches keep walking after the fork, so each has records.
        let src = r#"category C {
            objects: Alert, Latency, Security, Db, Keys;
            frame Alert: noul;
            latency:  Alert -> Latency  "slow responses";
            security: Alert -> Security "leaked credentials";
            db:   Latency -> Db    "database";
            keys: Security -> Keys "credentials";
            closed: Alert, Latency, Security, Db, Keys;
        }"#;
        let cat = Arc::new(onto_core::parse(src).unwrap());
        let cfg = Config {
            policy: Policy::Shared,
            ..Config::default()
        };
        let engine = Engine::new(
            cat,
            MockJudge { latency: LATENCY },
            MockProposer { latency: LATENCY },
            cfg,
        );
        let job = Job {
            from: "Alert".into(),
            goal: "slow database and leaked credentials".into(),
            case: json!({}),
        };
        let r = engine.run(vec![job]).await.unwrap();

        let (root, branch) = (&r.walks[0], &r.walks[1]);
        let fork = &root.frames[0];
        assert!(
            kinds(fork).contains(&(
                if branch.path.contains("Latency") {
                    "latency"
                } else {
                    "security"
                }
                .to_owned(),
                Disposition::Forked {
                    branch: Some(branch.walk)
                }
            ))
        );
        assert!(
            kinds(fork)
                .iter()
                .any(|(_, d)| *d == Disposition::Forked { branch: None })
        );
        assert!(
            matches!(fork.outcome, onto_runtime::record::Outcome::Forked { ref spawned, .. } if spawned == &[branch.walk])
        );
        // Causal links: the branch's first visit follows the fork; the root's
        // next visit follows it too.
        assert_eq!(branch.frames[0].after.as_deref(), Some(fork.id.as_str()));
        assert_eq!(root.frames[1].after.as_deref(), Some(fork.id.as_str()));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn branches_carry_their_focus_to_judges_and_proposers() {
        // Latency and Security are open with no arrows: each branch escalates
        // there, and its proposer must work on its own aspect only.
        let src = r#"category C {
            objects: Alert, Latency, Security;
            frame Alert: noul;
            latency:  Alert -> Latency  "slow responses";
            security: Alert -> Security "leaked credentials";
            closed: Alert;
        }"#;
        let cat = Arc::new(onto_core::parse(src).unwrap());
        let cfg = Config {
            policy: Policy::Shared,
            ..Config::default()
        };
        let engine = Engine::new(
            cat,
            MockJudge { latency: LATENCY },
            MockProposer { latency: LATENCY },
            cfg,
        );
        let job = Job {
            from: "Alert".into(),
            goal: "slow checkout and leaked credentials".into(),
            case: json!({}),
        };
        let r = engine.run(vec![job]).await.unwrap();
        assert_eq!(r.walks.len(), 2);
        for w in &r.walks {
            let visit = w.frames.last().unwrap();
            let focus = visit
                .focus
                .as_deref()
                .expect("both walks are branches after the fork");
            assert_eq!(
                visit.proposals[0].arrow,
                format!("to_{focus}"),
                "walk {} proposed off-focus",
                w.walk
            );
        }
        assert_eq!(
            r.walks[0].frames[0].focus, None,
            "the fork itself was visited before any focus"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn competing_readings_are_alternatives_with_a_reason() {
        let r = run_parts(
            4,
            "Alert",
            "slow responses, maybe leaked credentials",
            json!({}),
        )
        .await;
        let alt = r.walks[0].frames[0]
            .candidates
            .iter()
            .find(|c| c.disposition == Disposition::Alternative)
            .expect("one alternative");
        assert!(alt.reason.contains("competing reading"), "{}", alt.reason);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn require_failures_are_recorded_without_a_judge() {
        let r = run_parts(4, "Consented", "send me offers", json!({})).await;
        let f = &r.walks[0].frames[0];
        assert!(f.judge.is_none());
        assert_eq!(f.candidates[0].disposition, Disposition::FilteredByRequire);
        assert!(
            f.candidates[0].reason.contains("consent.marketing == true"),
            "{}",
            f.candidates[0].reason
        );
        assert_eq!(
            f.candidates[0].require.as_deref(),
            Some("consent.marketing == true")
        );
    }
}
