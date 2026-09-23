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
        open_world: false,
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
        open_world: false,
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
        open_world: false,
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
            open_world: false,
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
            open_world: false,
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

mod supervisor {
    use onto_core::supervise::{Admission, Outcome};
    use onto_core::walk::Proposal;
    use onto_runtime::model::MockCritic;
    use onto_runtime::supervisor::review;

    const CONSENT: &str = r#"
    category Consent {
        objects: Collected, Consented, Contract, Marketing;
        consent:  Collected -> Consented "explicit consent";
        contract: Collected -> Contract "necessary for the contract";
        market:   Consented -> Marketing "send offers";
        closed: Collected;
        invariant via: Collected -> Marketing through Consented;
        invariant rule "no arrow may introduce another legal basis such as legitimate interest";
    }"#;

    fn p(arrow: &str, src: &str, dst: &str, about: &str) -> Proposal {
        Proposal {
            arrow: arrow.into(),
            src: src.into(),
            dst: dst.into(),
            about: about.into(),
            rationale: String::new(),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn proven_rejects_never_ask_the_critic() {
        let cat = onto_core::parse(CONSENT).unwrap();
        let r = review(
            &cat,
            "r1".into(),
            vec![],
            &p("ads", "Collected", "Marketing", "ads"),
            &[],
            &MockCritic,
        )
        .await;
        assert_eq!(r.admission, Admission::Reject);
        assert!(r.critic.is_none());
        assert_eq!(
            r.checks
                .iter()
                .find(|c| c.outcome == Outcome::Fail)
                .unwrap()
                .witness
                .as_deref(),
            Some("ads")
        );
    }

    #[tokio::test]
    async fn rule_violations_are_rejected_by_the_critic() {
        let cat = onto_core::parse(CONSENT).unwrap();
        let r = review(
            &cat,
            "r1".into(),
            vec![],
            &p(
                "assess",
                "Collected",
                "Assessment",
                "legitimate interest assessment",
            ),
            &[],
            &MockCritic,
        )
        .await;
        assert_eq!(r.admission, Admission::Reject);
        assert!(
            r.checks
                .iter()
                .any(|c| c.check == "rule" && c.outcome == Outcome::Fail)
        );
        assert_eq!(r.critic.as_deref(), Some("mock-critic"));
    }

    #[tokio::test]
    async fn duplicates_of_earlier_proposals_are_rejected() {
        let cat = onto_core::parse(CONSENT).unwrap();
        let first = p("delivery", "Collected", "Delivery", "shipping data");
        let r = review(
            &cat,
            "r2".into(),
            vec![],
            &p("shipping", "Collected", "Delivery", "shipping data"),
            &[first],
            &MockCritic,
        )
        .await;
        assert_eq!(r.admission, Admission::Reject, "{:?}", r.checks);
        assert!(
            r.checks
                .iter()
                .any(|c| c.check == "duplicate" && c.subject.contains("proposed earlier"))
        );
    }

    #[tokio::test]
    async fn safe_proposals_are_admitted_with_every_check_recorded() {
        let cat = onto_core::parse(CONSENT).unwrap();
        let r = review(
            &cat,
            "r1".into(),
            vec!["w1.1".into()],
            &p(
                "newsletter",
                "Consented",
                "Newsletter",
                "send the newsletter",
            ),
            &[],
            &MockCritic,
        )
        .await;
        assert_eq!(r.admission, Admission::Admit, "{:?}", r.checks);
        let kinds: Vec<&str> = r.checks.iter().map(|c| c.check.as_str()).collect();
        assert!(
            kinds.contains(&"well_formed")
                && kinds.contains(&"invariant")
                && kinds.contains(&"rule")
        );
        assert!(kinds.contains(&"duplicate"));
        // Consented is a choice frame (the default), so overlap with its
        // sibling `market` is checked: overlap would break its MECE claim.
        assert!(kinds.contains(&"overlap"));
    }
}

mod contracts {
    use super::*;

    const SRC: &str = r#"category C {
        objects: Collected, Consented, Marketing;
        consent: Collected -> Consented "consent" require consent.given == true ensures ConsentGrant;
        market:  Consented -> Marketing "offers";
        entry Marketing: needs ConsentGrant;
        closed: Collected, Consented, Marketing;
        capability ConsentGrant { issuers: consent; }
    }"#;

    async fn walk(from: &str, goal: &str, case: Value) -> RunReport {
        let cat = Arc::new(onto_core::parse(SRC).unwrap());
        let cfg = Config {
            policy: Policy::Shared,
            open_world: false,
            ..Config::default()
        };
        let engine = Engine::new(
            cat,
            MockJudge { latency: LATENCY },
            MockProposer { latency: LATENCY },
            cfg,
        );
        engine
            .run(vec![Job {
                from: from.into(),
                goal: goal.into(),
                case,
            }])
            .await
            .unwrap()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn tokens_flow_from_checked_evidence_to_entry_contracts() {
        let ok = walk(
            "Collected",
            "consent then offers",
            json!({"consent": {"given": true}}),
        )
        .await;
        assert_eq!(ok.walks[0].path, "market.consent : Collected -> Marketing");
        assert_eq!(ok.walks[0].frames[1].tokens, ["ConsentGrant"]);

        // Starting past the evidence check: Consented can be started at (no
        // contract), but Marketing's contract blocks the walk.
        let skipped = walk("Consented", "offers", json!({"consent": {"given": true}})).await;
        let f = &skipped.walks[0].frames[0];
        assert_eq!(
            f.candidates[0].disposition,
            onto_core::walk::Disposition::BlockedByEntry
        );
        assert!(
            f.candidates[0].reason.contains("needs ConsentGrant"),
            "{}",
            f.candidates[0].reason
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_walk_cannot_start_inside_a_contract() {
        let r = walk("Marketing", "offers", json!({})).await;
        assert!(
            matches!(&r.walks[0].steps[0], StepRecord::Failed { error, .. } if error.contains("cannot start at Marketing"))
        );
        assert_eq!(r.judge_calls, 0);
    }
}

mod joins {
    use super::*;
    use onto_core::walk::Escalation;
    use onto_runtime::record::Outcome;

    /// Alert forks into Latency and Security (mock: goal contains " and ");
    /// each branch reaches Merge, gaining Seen or Checked on the way.
    fn src(join: &str, verify_require: &str) -> String {
        format!(
            r#"category C {{
            capability Seen {{ issuers: look; }}
            capability Checked {{ issuers: verify; }}
            objects: Alert, Latency, Security, Merge, Done;
            frame Alert: noul;
            latency:  Alert -> Latency  "slow responses";
            security: Alert -> Security "leaked credentials";
            look:   Latency -> Merge "merge" ensures Seen;
            verify: Security -> Merge "merge" {verify_require} ensures Checked;
            finish: Merge -> Done "done";
            closed: Alert, Latency, Security, Merge, Done;
            {join}
        }}"#
        )
    }

    async fn go(join: &str, verify_require: &str) -> RunReport {
        let cat = Arc::new(onto_core::parse(&src(join, verify_require)).unwrap());
        let cfg = Config {
            policy: Policy::Shared,
            open_world: false,
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
            goal: "slow and leaked credentials: merge, then done".into(),
            case: json!({}),
        };
        tokio::time::timeout(Duration::from_secs(5), engine.run(vec![job]))
            .await
            .expect("join deadlocked")
            .unwrap()
    }

    fn joined(w: &onto_runtime::engine::WalkReport) -> (&str, Vec<String>) {
        w.steps
            .iter()
            .find_map(|s| match s {
                StepRecord::Joined { role, tokens, .. } => Some((role.as_str(), tokens.clone())),
                _ => None,
            })
            .expect("a join step")
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn all_join_continues_once_with_the_intersection() {
        let r = go("join Merge: all;", "").await;
        assert_eq!(r.walks.len(), 2);
        let roles: Vec<&str> = r.walks.iter().map(|w| joined(w).0).collect();
        assert_eq!(roles.iter().filter(|r| **r == "continued").count(), 1);
        assert_eq!(roles.iter().filter(|r| **r == "ended").count(), 1);
        let cont = r.walks.iter().find(|w| joined(w).0 == "continued").unwrap();
        // Seen ∩ Checked = {}: least privilege.
        assert!(joined(cont).1.is_empty());
        assert!(cont.path.ends_with("-> Done"), "{}", cont.path);
        // The join record is a merge node of the disposition graph.
        let rec = cont
            .frames
            .iter()
            .find(|f| matches!(f.outcome, Outcome::Joined { .. }))
            .unwrap();
        assert_eq!(rec.merged_from.len(), 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn gate_join_exports_only_the_allowlist() {
        let r = go("join Merge: gate authority security export Checked;", "").await;
        let content = r.walks.iter().find(|w| joined(w).0 == "continued").unwrap();
        // (Seen ∩ Checked) ∪ (Checked ∩ {Checked}) = {Checked}
        assert_eq!(joined(content).1, ["Checked"]);
        let authority = r.walks.iter().find(|w| joined(w).0 == "ended").unwrap();
        assert!(authority.steps.iter().any(|s| matches!(s, StepRecord::Joined { detail, .. } if detail.contains("delivered authority"))));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn race_join_keeps_the_winner_and_its_own_tokens() {
        let r = go("join Merge: race;", "").await;
        let winner = r.walks.iter().find(|w| joined(w).0 == "continued").unwrap();
        let loser = r.walks.iter().find(|w| joined(w).0 == "ended").unwrap();
        assert_eq!(joined(winner).1.len(), 1, "the winner keeps its own token");
        assert!(loser.steps.iter().any(
            |s| matches!(s, StepRecord::Joined { detail, .. } if detail.contains("lost the race"))
        ));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn all_join_is_incomplete_when_a_sibling_ends_elsewhere() {
        // `verify` needs evidence the case lacks: the Security branch
        // escalates at Security and never arrives.
        let r = go("join Merge: all;", "require evidence == true").await;
        let waiting = r
            .walks
            .iter()
            .find(|w| {
                w.steps
                    .iter()
                    .any(|s| matches!(s, StepRecord::Joined { .. }))
            })
            .unwrap();
        assert!(waiting.steps.iter().any(|s| matches!(
            s,
            StepRecord::Escalated {
                reason: Escalation::IncompleteJoin,
                ..
            }
        )));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn gate_join_blocks_when_authority_never_arrives() {
        let r = go(
            "join Merge: gate authority security export Checked;",
            "require evidence == true",
        )
        .await;
        let content = r
            .walks
            .iter()
            .find(|w| {
                w.steps
                    .iter()
                    .any(|s| matches!(s, StepRecord::Joined { .. }))
            })
            .unwrap();
        assert!(content.steps.iter().any(|s| matches!(
            s,
            StepRecord::Escalated {
                reason: Escalation::BlockedByGate,
                ..
            }
        )));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn every_record_names_its_snapshot() {
    let r = run(Policy::Shared, false, &[FIX]).await;
    let expected = onto_core::parse::snapshot_hash(TRIAGE);
    assert_eq!(r.snapshot.as_deref(), Some(expected.as_str()));
    assert!(
        r.walks[0]
            .frames
            .iter()
            .all(|f| f.snapshot.as_deref() == Some(expected.as_str()))
    );
}

mod attested_joins {
    use super::*;
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD as B64;
    use ed25519_dalek::{Signer, SigningKey};

    fn signer() -> SigningKey {
        SigningKey::from_bytes(&[7; 32])
    }

    /// Both branches reach Merge; `look` needs an attested observation,
    /// `verify` optionally too.
    fn src(verify_attested: bool) -> String {
        format!(
            r#"category C {{
            attester Ops {{ key: ed25519:{}; observes: latency.fixed, keys.revoked; }}
            objects: Alert, Latency, Security, Merge, Done;
            frame Alert: noul;
            latency:  Alert -> Latency  "slow responses";
            security: Alert -> Security "leaked credentials";
            look:   Latency -> Merge "merge" attested latency.fixed == true;
            verify: Security -> Merge "merge" {};
            finish: Merge -> Done "done";
            closed: Alert, Latency, Security, Merge, Done;
            join Merge: all;
        }}"#,
            B64.encode(signer().verifying_key().to_bytes()),
            if verify_attested {
                "attested keys.revoked == true"
            } else {
                ""
            }
        )
    }

    fn obs(field: &str) -> Value {
        let claim = json!({ field: true }).as_object().unwrap().clone();
        let sig = signer().sign(&onto_core::attest::message("Ops", "INC-9", "t0", &claim));
        json!({"claim": claim, "attester": "Ops", "case": "INC-9", "at": "t0", "signature": B64.encode(sig.to_bytes())})
    }

    async fn go(verify_attested: bool) -> (bool, String) {
        let cat = Arc::new(onto_core::parse(&src(verify_attested)).unwrap());
        let cfg = Config {
            policy: Policy::Shared,
            open_world: false,
            ..Config::default()
        };
        let engine = Engine::new(
            cat,
            MockJudge { latency: LATENCY },
            MockProposer { latency: LATENCY },
            cfg,
        );
        let case =
            json!({"id": "INC-9", "observations": [obs("latency.fixed"), obs("keys.revoked")]});
        let job = Job {
            from: "Alert".into(),
            goal: "slow and leaked credentials: merge, then done".into(),
            case,
        };
        let r = tokio::time::timeout(Duration::from_secs(5), engine.run(vec![job]))
            .await
            .unwrap()
            .unwrap();
        r.walks
            .iter()
            .flat_map(|w| &w.steps)
            .find_map(|s| match s {
                StepRecord::Joined {
                    role,
                    completion_attested,
                    detail,
                    ..
                } if role == "continued" => Some((*completion_attested, detail.clone())),
                _ => None,
            })
            .expect("a continuing join")
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_join_is_attested_completion_only_if_every_branch_arrived_attested() {
        let (attested, detail) = go(true).await;
        assert!(attested, "{detail}");
        assert!(
            detail.contains("completion attested") && detail.contains("Ops: "),
            "{detail}"
        );

        let (attested, detail) = go(false).await;
        assert!(!attested);
        assert!(detail.contains("NOT attested"), "{detail}");
    }
}

mod parallel_race {
    use super::*;
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD as B64;
    use ed25519_dalek::{Signer, SigningKey};

    #[tokio::test(flavor = "multi_thread")]
    async fn parallel_plans_race_and_only_attested_ones_can_win() {
        let k = SigningKey::from_bytes(&[9; 32]);
        let src = format!(
            r#"category C {{
            attester Ops {{ key: ed25519:{}; observes: a.done, b.done; }}
            objects: Outage, PlanA, PlanB, Mitigating;
            frame Outage: noul parallel;
            try_a: Outage -> PlanA "slow";
            try_b: Outage -> PlanB "leaked";
            done_a: PlanA -> Mitigating "mitigate" attested a.done == true;
            done_b: PlanB -> Mitigating "mitigate" attested b.done == true;
            closed: Outage, PlanA, PlanB, Mitigating;
            join Mitigating: race;
        }}"#,
            B64.encode(k.verifying_key().to_bytes())
        );
        let claim = json!({"a.done": true}).as_object().unwrap().clone();
        let sig = k.sign(&onto_core::attest::message("Ops", "I-1", "t", &claim));
        let obs = json!({"claim": claim, "attester": "Ops", "case": "I-1", "at": "t", "signature": B64.encode(sig.to_bytes())});
        let cat = Arc::new(onto_core::parse(&src).unwrap());
        let engine = Engine::new(
            cat,
            MockJudge { latency: LATENCY },
            MockProposer { latency: LATENCY },
            Config {
                policy: Policy::Shared,
                open_world: false,
                ..Config::default()
            },
        );
        // No " and " in the goal: a non-parallel frame would not fork.
        let job = Job {
            from: "Outage".into(),
            goal: "slow, leaked: mitigate".into(),
            case: json!({"id": "I-1", "observations": [obs]}),
        };
        let r = tokio::time::timeout(Duration::from_secs(5), engine.run(vec![job]))
            .await
            .unwrap()
            .unwrap();

        assert_eq!(r.forks, 1, "parallel frames fork without a fork question");
        assert_eq!(
            r.judge_questions,
            2 + 1 + 1,
            "Outage: one noul per plan; PlanA, PlanB: one each; no fork question"
        );
        let winner = r
            .walks
            .iter()
            .find(|w| {
                w.steps
                    .iter()
                    .any(|s| matches!(s, StepRecord::Joined { role, .. } if role == "continued"))
            })
            .expect("a winner");
        assert!(winner.path.contains("done_a"), "{}", winner.path);
        let other = r.walks.iter().find(|w| w.walk != winner.walk).unwrap();
        let blocked = other.frames.last().unwrap();
        assert_eq!(
            blocked.candidates[0].disposition,
            onto_core::walk::Disposition::Unattested
        );
    }
}

mod split_frames {
    use super::*;
    use onto_core::walk::Escalation;

    const SRC: &str = r#"category C {
        objects: Mitigating, E, L, K, Verified;
        frame Mitigating: split;
        check_e: Mitigating -> E;
        check_l: Mitigating -> L;
        check_k: Mitigating -> K;
        e_ok: E -> Verified "errors";
        l_ok: L -> Verified "latency";
        k_ok: K -> Verified "checkout";
        closed: Mitigating, E, L, K, Verified;
        join Verified: all;
    }"#;

    async fn go(max_branches: usize) -> RunReport {
        let cat = Arc::new(onto_core::parse(SRC).unwrap());
        let cfg = Config {
            policy: Policy::Shared,
            max_branches,
            open_world: false,
            ..Config::default()
        };
        let engine = Engine::new(
            cat,
            MockJudge { latency: LATENCY },
            MockProposer { latency: LATENCY },
            cfg,
        );
        let job = Job {
            from: "Mitigating".into(),
            goal: "errors latency checkout".into(),
            case: json!({}),
        };
        tokio::time::timeout(Duration::from_secs(5), engine.run(vec![job]))
            .await
            .unwrap()
            .unwrap()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_split_pursues_every_arrow_without_judgment() {
        let r = go(4).await;
        assert_eq!((r.forks, r.branches, r.walks.len()), (1, 2, 3));
        // Judged only the three check frames (E, L, K), never Mitigating.
        assert_eq!(r.judge_calls, 3);
        assert!(r.walks[0].frames[0].judge.is_none());
        assert!(r.walks.iter().any(|w| {
            w.steps
                .iter()
                .any(|s| matches!(s, StepRecord::Joined { role, .. } if role == "continued"))
        }));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_split_over_budget_escalates_instead_of_dropping_a_check() {
        let r = go(1).await;
        assert_eq!(r.walks.len(), 1, "nothing forked");
        assert!(matches!(
            r.walks[0].steps[0],
            StepRecord::Escalated {
                reason: Escalation::SplitOverBudget,
                ..
            }
        ));
        assert_eq!(r.proposer_calls, 0);
    }
}

mod sequential_joins {
    use super::*;

    /// Race at M, then a split into checks that must all arrive at V: the
    /// race winner must count as running again for the second join. The
    /// winner's own check path (E, E2) is one hop longer, so the other
    /// checks reach V while the winner is still walking.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_race_winner_can_complete_a_later_all_join() {
        let src = r#"category C {
            objects: Outage, A, B, M, E, E2, L, V;
            frame Outage: noul parallel;
            try_a: Outage -> A "slow";
            try_b: Outage -> B "leaked";
            a_done: A -> M "mitigate";
            b_done: B -> M "mitigate";
            join M: race;
            frame M: split;
            check_e: M -> E;
            check_l: M -> L;
            e_mid: E -> E2 "errors";
            e_ok: E2 -> V "errors";
            l_ok: L -> V "latency";
            join V: all;
            closed: Outage, A, B, M, E, E2, L, V;
        }"#;
        let cat = Arc::new(onto_core::parse(src).unwrap());
        let cfg = Config {
            policy: Policy::Shared,
            max_branches: 6,
            open_world: false,
            ..Config::default()
        };
        let engine = Engine::new(
            cat,
            MockJudge { latency: LATENCY },
            MockProposer { latency: LATENCY },
            cfg,
        );
        let job = Job {
            from: "Outage".into(),
            goal: "slow, leaked: mitigate errors latency".into(),
            case: json!({}),
        };
        let r = tokio::time::timeout(Duration::from_secs(5), engine.run(vec![job]))
            .await
            .unwrap()
            .unwrap();
        let continued: Vec<&str> = r
            .walks
            .iter()
            .flat_map(|w| &w.steps)
            .filter_map(|s| match s {
                StepRecord::Joined { role, policy, .. } if role == "continued" => {
                    Some(policy.as_str())
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            continued,
            ["race", "all"],
            "{:#?}",
            r.walks.iter().map(|w| &w.steps).collect::<Vec<_>>()
        );
    }
}

/// Open world (the default): a missing enumeration is filled by System 2,
/// the supervisor admits what it cannot reject, and the walk continues.
mod open_world {
    use super::*;
    use onto_core::walk::Proposal;
    use onto_runtime::model::{ModelError, ProposalRequest, Proposer, Usage};

    /// Proposes the same arrows at every escalation.
    struct Fixed(Vec<Proposal>);

    impl Proposer for Fixed {
        fn name(&self) -> String {
            "fixed".into()
        }
        async fn propose(&self, _: ProposalRequest) -> Result<(Vec<Proposal>, Usage), ModelError> {
            Ok((self.0.clone(), Usage::default()))
        }
    }

    fn arrow(name: &str, src: &str, dst: &str) -> Proposal {
        Proposal {
            arrow: name.into(),
            src: src.into(),
            dst: dst.into(),
            about: format!("cases for {dst}"),
            ..Default::default()
        }
    }

    async fn walk<P: Proposer>(src: &str, from: &str, goal: &str, p: P, cfg: Config) -> RunReport {
        let cat = Arc::new(onto_core::parse(src).unwrap());
        let engine = Engine::new(cat, MockJudge { latency: LATENCY }, p, cfg);
        let job = Job {
            from: from.into(),
            goal: goal.into(),
            case: json!({}),
        };
        engine.run(vec![job]).await.unwrap()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn missing_enumeration_is_learned_and_the_walk_continues() {
        let r = walk(
            TRIAGE,
            "Request",
            "please refund",
            MockProposer { latency: LATENCY },
            Config::default(),
        )
        .await;
        let steps = &r.walks[0].steps;
        let StepRecord::Expanded { at, learned, .. } = &steps[0] else {
            panic!("expected expansion: {steps:?}");
        };
        assert_eq!(
            (at.as_str(), learned[0].dst.as_str()),
            ("Request", "Refund")
        );
        let StepRecord::Followed { to, learned, .. } = &steps[1] else {
            panic!("expected the walk to follow the learned arrow: {steps:?}");
        };
        assert_eq!(to, "Refund");
        assert!(learned);
        // At the new object the proposer repeats itself: refused as a
        // duplicate name, so the walk escalates instead of looping.
        assert!(matches!(steps.last(), Some(StepRecord::Escalated { .. })));
        assert_eq!(r.learned.len(), 1);
        assert!(r.walks[0].path.ends_with("Request -> Refund"));
        // The frame record says it was expanded; the learned candidate is marked.
        let f = &r.walks[0].frames;
        assert!(matches!(
            f[0].outcome,
            onto_runtime::record::Outcome::Expanded { .. }
        ));
        // The re-judged visit at Request offers the learned arrow, marked.
        assert_eq!(f[1].at, "Request");
        assert!(
            f[1].candidates
                .iter()
                .any(|c| c.learned && c.arrow == "to_refund")
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn closed_world_and_zero_budget_stop_at_escalation() {
        for cfg in [
            Config {
                open_world: false,
                ..Config::default()
            },
            Config {
                max_expansions: 0,
                ..Config::default()
            },
        ] {
            let r = walk(
                TRIAGE,
                "Request",
                "please refund",
                MockProposer { latency: LATENCY },
                cfg,
            )
            .await;
            assert!(matches!(r.walks[0].steps[0], StepRecord::Escalated { .. }));
            assert!(r.learned.is_empty());
        }
    }

    const CONSENT: &str = r#"
    category C {
        objects: Collected, Consented, Marketing, Archive;
        capability Grant { issuers: consent; }
        consent: Collected -> Consented;
        market:  Consented -> Marketing;
        entry Marketing: needs Grant;
        invariant via: Collected -> Marketing through Consented;
    }
    "#;

    #[tokio::test(flavor = "multi_thread")]
    async fn policy_violations_are_never_learned() {
        let bypass = arrow("shortcut", "Collected", "Marketing");
        let forged = Proposal {
            ensures: vec!["Grant".into()],
            ..arrow("forge", "Collected", "Archive")
        };
        let r = walk(
            CONSENT,
            "Collected",
            "archive it",
            Fixed(vec![bypass, forged]),
            Config::default(),
        )
        .await;
        assert!(r.learned.is_empty(), "{:?}", r.learned);
        let StepRecord::Escalated { .. } = &r.walks[0].steps[0] else {
            panic!("expected a stop: {:?}", r.walks[0].steps);
        };
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_valid_proposal_is_learned_beside_a_refused_one() {
        let bypass = arrow("shortcut", "Collected", "Marketing");
        let archive = arrow("archive", "Collected", "Archive");
        let r = walk(
            CONSENT,
            "Collected",
            "archive it",
            Fixed(vec![bypass, archive]),
            Config::default(),
        )
        .await;
        let StepRecord::Expanded {
            learned, refused, ..
        } = &r.walks[0].steps[0]
        else {
            panic!("expected expansion: {:?}", r.walks[0].steps);
        };
        assert_eq!(learned.len(), 1);
        assert_eq!(learned[0].arrow, "archive");
        assert_eq!(refused[0].0, "shortcut");
        assert!(refused[0].1.starts_with("invariant"), "{refused:?}");
    }
}

/// `state` declarations decide exactly what a model is shown, and the
/// record keeps it.
#[tokio::test(flavor = "multi_thread")]
async fn models_see_only_the_declared_state() {
    let src = r#"category C {
        objects: Use, Marketing, Research, Sent;
        state { goal; case: request.purpose; }
        state Marketing { case: request.purpose; history: last 1; }
        marketing: Use -> Marketing "sending offers";
        research:  Use -> Research  "a study";
        send: Marketing -> Sent "send the offers";
        closed: Use, Marketing, Sent;
        invariant unseen: case.person;
    }"#;
    let cat = Arc::new(onto_core::parse(src).unwrap());
    let engine = Engine::new(
        cat,
        MockJudge { latency: LATENCY },
        MockProposer { latency: LATENCY },
        Config {
            open_world: false,
            ..Config::default()
        },
    );
    let job = Job {
        from: "Use".into(),
        goal: "send offers to marketing".into(),
        case: json!({
            "goal": "send offers to marketing",
            "request": {"purpose": "marketing", "requested_by": "growth"},
            "person": {"name": "Ayşe", "health": "diabetes"},
        }),
    };
    let r = engine.run(vec![job]).await.unwrap();
    let f = &r.walks[0].frames;
    let first = f[0].seen.as_ref().expect("the judge was asked");
    assert_eq!(
        first["asserted"],
        json!({"request": {"purpose": "marketing"}})
    );
    assert_eq!(first["goal"], "send offers to marketing");
    assert!(first.get("inferred").is_none());
    // Marketing's own declaration: no goal, one hop of history.
    let second = f[1].seen.as_ref().expect("the judge was asked");
    assert!(second.get("goal").is_none());
    assert_eq!(second["inferred"]["hops"].as_array().unwrap().len(), 1);
    for rec in f {
        let text = serde_json::to_string(&rec.seen).unwrap();
        assert!(
            !text.contains("Ayşe") && !text.contains("diabetes"),
            "{text}"
        );
    }
}

/// Sealed frames are never extended by the open world, and learned arrows
/// may not enter them; `world: closed` seals everything not declared
/// learnable.
mod sealed {
    use super::*;

    async fn walk(src: &str, goal: &str) -> RunReport {
        let cat = Arc::new(onto_core::parse(src).unwrap());
        let engine = Engine::new(
            cat,
            MockJudge { latency: LATENCY },
            MockProposer { latency: LATENCY },
            Config::default(),
        );
        let job = Job {
            from: "Request".into(),
            goal: goal.into(),
            case: json!({}),
        };
        engine.run(vec![job]).await.unwrap()
    }

    const TRIAGE_SEALED: &str = r#"category T {
        objects: Request, Bug, Done;
        report: Request -> Bug;
        fix: Bug -> Done;
        closed: Request, Bug, Done;
        sealed: Request;
    }"#;

    #[tokio::test(flavor = "multi_thread")]
    async fn a_sealed_frame_escalates_to_a_person() {
        let r = walk(TRIAGE_SEALED, "please refund").await;
        assert!(matches!(r.walks[0].steps[0], StepRecord::Escalated { .. }));
        assert!(r.learned.is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn closed_world_category_learns_only_where_declared() {
        let src = TRIAGE_SEALED.replace("sealed: Request;", "world: closed; learnable: Request;");
        let r = walk(&src, "please refund").await;
        // Request is learnable; the new object Refund is too (the open
        // world made it), so the walk goes on past it.
        assert!(matches!(r.walks[0].steps[0], StepRecord::Expanded { .. }));
        assert_eq!(r.learned[0].proposal.dst, "Refund");
        // Bug stays sealed: no learned arrow may enter or leave it.
        let cat = onto_core::parse(&src).unwrap();
        let into_bug = onto_core::walk::Proposal {
            arrow: "escalate".into(),
            src: "Request".into(),
            dst: "Bug".into(),
            ..Default::default()
        };
        let (_, skipped) = cat.with_learned(&[into_bug]);
        assert!(skipped[0].1.starts_with("sealed: Bug"), "{skipped:?}");
    }
}

/// Precedents are projected onto the frame's current declaration: a field
/// the frame may not see never arrives through memory, and a case is never
/// its own precedent.
#[tokio::test(flavor = "multi_thread")]
async fn memory_shows_projected_precedents_from_other_cases() {
    use onto_runtime::memory::Precedent;
    let src = r#"category C {
        objects: Circumstances, Disability, Done;
        state Circumstances { goal; case: statement; memory: similar 2; }
        state { goal; }
        has_disability: Circumstances -> Disability "a disability is described";
        none_apply:     Circumstances -> Done "no disability is described";
        closed: Circumstances;
        invariant unseen: case.applicant;
    }"#;
    let cat = Arc::new(onto_core::parse(src).unwrap());
    let engine = Engine::new(
        cat,
        MockJudge { latency: LATENCY },
        MockProposer { latency: LATENCY },
        Config {
            open_world: false,
            ..Config::default()
        },
    );
    let precedent = |case: &str, statement: &str| Precedent {
        frame: "Circumstances".into(),
        record: format!("w0.{case}"),
        snapshot: None,
        case: Some(case.into()),
        // Written under an older, leakier policy: carries the applicant.
        saw: json!({"asserted": {"statement": statement, "applicant": {"name": "X"}}}),
        decided: "has_disability".into(),
        p: 0.9,
    };
    engine.remember(vec![
        precedent("B-1", "chronic back pain, cannot lift"),
        precedent("B-2", "back pain after an accident, cannot lift"),
    ]);
    let job = Job {
        from: "Circumstances".into(),
        goal: "a disability: back pain, cannot lift".into(),
        case: json!({"id": "B-2", "statement": "back pain, cannot lift", "applicant": {"name": "Y"}}),
    };
    let r = engine.run(vec![job]).await.unwrap();
    let seen = r.walks[0].frames[0].seen.as_ref().unwrap();
    let cases = seen["precedents"]["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 1, "B-2 is not its own precedent: {seen}");
    assert_eq!(
        cases[0]["saw"],
        json!({"asserted": {"statement": "chronic back pain, cannot lift"}})
    );
    assert_eq!(cases[0]["to"], "Disability");
    assert!(!seen.to_string().contains("applicant"));
    // This decision becomes a precedent, projected the same way.
    assert_eq!(r.precedents.len(), 1);
    assert!(!r.precedents[0].saw.to_string().contains("applicant"));
}

/// Functors in a walk: transport completes a missing enumeration from a
/// catalogue before any LLM; `grouped by` judges a coarse group first.
mod functors {
    use super::*;
    use onto_runtime::lens::Lens;

    fn lenses(src: &str) -> (Arc<onto_core::Category>, Vec<Lens>) {
        let m =
            onto_core::parse_module(src, &mut |p| Err(onto_core::Error::UnknownObject(p.into())))
                .unwrap();
        let cat = Arc::new(m.categories[0].clone());
        let ls = m
            .functors
            .iter()
            .map(|f| Lens {
                functor: f.clone(),
                target: Arc::new(m.category(&f.dst).unwrap().clone()),
            })
            .collect();
        (cat, ls)
    }

    const TRANSPORT: &str = r#"
    category Benefits {
        objects: Circumstances, Disability, Done;
        frame Circumstances: choice "Which circumstance?";
        has_disability: Circumstances -> Disability "own disability";
        none_apply: Circumstances -> Done "nothing special";
        go: Disability -> Done;
        closed: Circumstances, Disability, Done;
    }
    category Catalogue {
        objects: Circ, Dis, Mat, Assessed;
        disability: Circ -> Dis "own disability";
        maternity: Circ -> Mat "pregnancy or a new baby";
        none: Circ -> Assessed "nothing special";
        dis_ok: Dis -> Assessed;
        maternity_evidenced: Mat -> Assessed "maternity is confirmed";
        closed: Circ;
    }
    functor Catalog: Benefits -> Catalogue {
        objects: Circumstances -> Circ, Disability -> Dis, Done -> Assessed;
        has_disability: disability; none_apply: none; go: dis_ok;
        transport;
    }
    "#;

    #[tokio::test(flavor = "multi_thread")]
    async fn transport_completes_the_enumeration_without_an_llm() {
        let (cat, ls) = lenses(TRANSPORT);
        let engine = Engine::new(
            cat,
            MockJudge { latency: LATENCY },
            MockProposer { latency: LATENCY },
            Config::default(),
        );
        engine.use_lenses(ls, &[]);
        let job = Job {
            from: "Circumstances".into(),
            goal: "i am pregnant, maternity".into(),
            case: json!({}),
        };
        let r = engine.run(vec![job]).await.unwrap();
        let steps = &r.walks[0].steps;
        let StepRecord::Expanded {
            source, learned, ..
        } = &steps[0]
        else {
            panic!("expected transport: {steps:?}");
        };
        assert_eq!(source, "transport Catalog");
        assert_eq!(learned[0].arrow, "maternity");
        assert_eq!(learned[0].dst, "Mat");
        assert!(matches!(&steps[1], StepRecord::Followed { arrow, .. } if arrow == "maternity"));
        // At the new object the catalogue knows the next step too: the walk
        // ends at a declared outcome, and System 2 was never asked.
        assert!(r.walks[0].path.ends_with("-> Done"), "{}", r.walks[0].path);
        assert_eq!(r.proposer_calls, 0);
        assert_eq!(
            r.learned[0].transported.as_deref(),
            Some("Catalog:maternity")
        );
    }

    const GROUPED: &str = r#"
    category Support {
        objects: Ticket, Refund, Invoice, Outage, HowTo;
        frame Ticket: choice grouped by Teams "Which kind of ticket?";
        refund: Ticket -> Refund "money back";
        invoice: Ticket -> Invoice "a billing document";
        outage: Ticket -> Outage "something is broken";
        howto: Ticket -> HowTo "how to use it";
        closed: Ticket, Refund, Invoice, Outage, HowTo;
    }
    category Coarse {
        objects: T, B, E;
        billing: T -> B "money: charges, refunds, invoices";
        technical: T -> E "the product misbehaves or someone needs help";
    }
    functor Teams: Support -> Coarse {
        objects: Ticket -> T, Refund -> B, Invoice -> B, Outage -> E, HowTo -> E;
        refund: billing; invoice: billing; outage: technical; howto: technical;
    }
    "#;

    #[tokio::test(flavor = "multi_thread")]
    async fn a_grouped_frame_judges_the_group_then_the_fiber() {
        let (cat, ls) = lenses(GROUPED);
        let engine = Engine::new(
            cat,
            MockJudge { latency: LATENCY },
            MockProposer { latency: LATENCY },
            Config {
                open_world: false,
                ..Config::default()
            },
        );
        engine.use_lenses(ls, &[]);
        let job = Job {
            from: "Ticket".into(),
            goal: "billing: i want a refund".into(),
            case: json!({}),
        };
        let r = engine.run(vec![job]).await.unwrap();
        let f = &r.walks[0].frames[0];
        let g = f.grouped.as_ref().expect("grouped");
        assert_eq!(g.groups, ["billing", "technical"]);
        assert_eq!(g.chose.as_deref(), Some("billing"));
        assert_eq!(g.kept, 2);
        let judged = f.candidates.iter().filter(|c| c.judgment.is_some()).count();
        assert_eq!(judged, 2, "only the fiber is judged");
        assert!(f.candidates.iter().any(|c| c.disposition
            == onto_core::walk::Disposition::OtherGroup
            && c.arrow == "outage"));
        assert_eq!(r.walks[0].path, "refund : Ticket -> Refund");
        assert_eq!(r.judge_calls, 2);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_wrong_group_backs_off_to_the_whole_frame() {
        let (cat, ls) = lenses(GROUPED);
        let engine = Engine::new(
            cat,
            MockJudge { latency: LATENCY },
            MockProposer { latency: LATENCY },
            Config {
                open_world: false,
                ..Config::default()
            },
        );
        engine.use_lenses(ls, &[]);
        // "charges" sends the coarse step to billing, but the intent is an
        // outage: the billing fiber holds nothing, so the frame is judged
        // again without grouping.
        let job = Job {
            from: "Ticket".into(),
            goal: "charges page shows an outage".into(),
            case: json!({}),
        };
        let r = engine.run(vec![job]).await.unwrap();
        let f = &r.walks[0].frames;
        assert_eq!(
            f[0].grouped.as_ref().unwrap().chose.as_deref(),
            Some("billing")
        );
        assert!(f[1].grouped.is_none(), "the second visit is flat");
        assert_eq!(r.walks[0].path, "outage : Ticket -> Outage");
    }
}

/// Two admission regimes over one runtime: open-world learning takes
/// what no hard check refutes; assured evolution takes only what every
/// check passed and holds the undecided for a person. Admission is per
/// frame; a run can tighten it, never loosen it.
mod loops {
    use super::*;
    use onto_core::walk::Answer;
    use onto_runtime::model::{Critic, FrameRequest, Judge, ModelError, NoulQuestion, Usage};

    /// Judges like the mock; as a critic it is never sure (p 0.5).
    struct Unsure(MockJudge);

    impl Judge for Unsure {
        fn name(&self) -> String {
            "unsure".into()
        }
        async fn judge(&self, req: FrameRequest) -> Result<(Answer, Usage), ModelError> {
            self.0.judge(req).await
        }
    }

    impl Critic for Unsure {
        fn name(&self) -> String {
            "unsure".into()
        }
        async fn nouls(
            &self,
            _: Value,
            questions: Vec<NoulQuestion>,
        ) -> Result<(Vec<f32>, Usage), ModelError> {
            Ok((vec![0.5; questions.len()], Usage::default()))
        }
    }

    async fn run(src: &str, assured: bool) -> RunReport {
        let cat = Arc::new(onto_core::parse(src).unwrap());
        let engine = Engine::new(
            cat,
            Unsure(MockJudge { latency: LATENCY }),
            MockProposer { latency: LATENCY },
            Config {
                assured,
                ..Config::default()
            },
        );
        let job = Job {
            from: "Request".into(),
            goal: "please refund".into(),
            case: json!({}),
        };
        engine.run(vec![job]).await.unwrap()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn open_world_learns_the_undecided_assured_holds_it() {
        let open = run(TRIAGE, false).await;
        assert!(matches!(
            open.walks[0].steps[0],
            StepRecord::Expanded { .. }
        ));
        assert_eq!(open.learned.len(), 1);

        let assured = run(TRIAGE, true).await;
        let StepRecord::Escalated { held, .. } = &assured.walks[0].steps[0] else {
            panic!("expected a hold: {:?}", assured.walks[0].steps);
        };
        assert_eq!(held[0].0, "to_refund");
        assert!(held[0].1.contains("unsure"), "{held:?}");
        assert!(assured.learned.is_empty());
        assert_eq!(assured.walks[0].frames[0].held.len(), 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn admission_is_per_frame() {
        // Request is assured: the undecided proposal waits for a person,
        // although the run itself does not force assured admission.
        let src = TRIAGE.replace(
            "closed: Request;",
            "closed: Request;\n    admission Request: assured;",
        );
        let r = run(&src, false).await;
        assert!(r.learned.is_empty());
        assert!(matches!(r.walks[0].steps[0], StepRecord::Escalated { .. }));
        // An assured default, with one frame opened, learns there.
        let src = TRIAGE.replace(
            "closed: Request;",
            "closed: Request;\n    admission: assured;\n    admission Request: open_world;",
        );
        let r = run(&src, false).await;
        assert_eq!(r.learned.len(), 1);
        // Sealed: nothing is learned, nothing is even reviewed.
        let src = TRIAGE.replace(
            "closed: Request;",
            "closed: Request;\n    admission Request: sealed;",
        );
        let r = run(&src, false).await;
        assert!(r.learned.is_empty());
    }
}
