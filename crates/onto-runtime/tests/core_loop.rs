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
