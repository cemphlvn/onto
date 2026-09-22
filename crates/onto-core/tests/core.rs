use onto_core::walk::{
    Answer, Decision, Distribution, Escalation, NullProposer, ScriptedJudge, Step, UniformJudge,
    Walker, decide,
};
use onto_core::{Closure, Primitive};
use onto_core::{Equality, Error, Verdict, category::resolve, parse, parse::path_spec};
use serde_json::json;

const TRIAGE: &str = include_str!("../../../examples/triage.onto");

const GROUPOIDISH: &str = "
category C {
    objects: A, B;
    f:   A -> B;
    inv: B -> A;
    inv.f = id(A);
}";

fn path(cat: &onto_core::Category, s: &str) -> onto_core::Path {
    resolve(cat, &path_spec(s).unwrap()).unwrap()
}

#[test]
fn frames_are_contiguous_in_declaration_order() {
    let cat = parse(TRIAGE).unwrap();
    let names = |obj: &str| -> Vec<&str> {
        cat.out(cat.object_id(obj).unwrap())
            .iter()
            .map(|a| cat.arrow(*a).name.as_str())
            .collect()
    };
    assert_eq!(names("Request"), ["ask", "report", "wish"]);
    assert_eq!(names("Bug"), ["patch", "wontfix"]);
    assert!(names("Done").is_empty());
}

#[test]
fn composition_is_type_checked() {
    let cat = parse(TRIAGE).unwrap();
    assert_eq!(
        path(&cat, "ship.patch.report").display_typed(&cat),
        "ship.patch.report : Request -> Done"
    );
    let err = resolve(&cat, &path_spec("patch.ask").unwrap()).unwrap_err();
    assert!(matches!(err, Error::NotComposable { .. }), "{err}");
}

#[test]
fn declared_equation_holds_in_context() {
    let cat = parse(TRIAGE).unwrap();
    let eq = Equality::new(&cat).unwrap();
    assert_eq!(
        eq.check(
            &path(&cat, "build.plan.wish"),
            &path(&cat, "implement.specify.wish")
        ),
        Verdict::Equal
    );
    assert_eq!(
        eq.check(&path(&cat, "ship.patch"), &path(&cat, "wontfix")),
        Verdict::Distinct
    );
}

#[test]
fn identity_laws_follow_from_equations() {
    let cat = parse(GROUPOIDISH).unwrap();
    let eq = Equality::new(&cat).unwrap();
    assert_eq!(
        eq.check(&path(&cat, "f.inv.f"), &path(&cat, "f")),
        Verdict::Equal
    );
    assert_eq!(eq.simplest(&path(&cat, "f.inv.f")), path(&cat, "f"));
    assert!(eq.simplest(&path(&cat, "inv.f")).is_id());
    // Nothing says f.inv = id(B).
    assert_ne!(
        eq.check(&path(&cat, "f.inv"), &path(&cat, "id(B)")),
        Verdict::Equal
    );
}

#[test]
fn equations_must_be_parallel() {
    let src = "category C { objects: A, B; f: A -> B; g: A -> A; f = g; }";
    assert!(matches!(parse(src), Err(Error::EquationNotParallel { .. })));
}

#[test]
fn parse_errors_carry_lines() {
    let src = "category C {\n objects: A;\n f: A -> Nowhere;\n}";
    let err = parse(src).unwrap_err();
    assert!(matches!(err, Error::Parse { line: 3, .. }), "{err}");
}

#[test]
fn reserved_names_are_rejected() {
    assert!(parse("category C { objects: id; }").is_err());
}

#[test]
fn walk_follows_closed_frames_to_a_terminal() {
    let cat = parse(TRIAGE).unwrap();
    let mut w = Walker {
        cat: &cat,
        judge: ScriptedJudge::new(["report", "patch", "ship"]),
        proposer: NullProposer,
        threshold: 0.5,
    };
    let walk = w.walk(
        "login page crashes",
        json!({}),
        cat.object_id("Request").unwrap(),
        10,
    );
    assert_eq!(walk.steps.len(), 3);
    assert_eq!(walk.state.path.display(&cat), "ship.patch.report");
}

#[test]
fn walk_escalates_on_open_frame() {
    let cat = parse(TRIAGE).unwrap();
    let mut w = Walker {
        cat: &cat,
        judge: ScriptedJudge::new(["ask"]),
        proposer: NullProposer,
        threshold: 0.5,
    };
    let walk = w.walk("", json!({}), cat.object_id("Request").unwrap(), 10);
    assert!(matches!(
        walk.steps.last(),
        Some(Step::Escalated {
            reason: Escalation::OpenFrame,
            ..
        })
    ));
    assert_eq!(walk.state.path.display(&cat), "ask");
}

#[test]
fn walk_escalates_when_system1_declines() {
    let cat = parse(TRIAGE).unwrap();
    let request = cat.object_id("Request").unwrap();

    let mut scripted = Walker {
        cat: &cat,
        judge: ScriptedJudge::new(["refund"]),
        proposer: NullProposer,
        threshold: 0.5,
    };
    assert!(matches!(
        scripted.walk("", json!({}), request, 10).steps[0],
        Step::Escalated {
            reason: Escalation::NoneOfThese,
            ..
        }
    ));

    let mut uniform = Walker {
        cat: &cat,
        judge: UniformJudge,
        proposer: NullProposer,
        threshold: 0.5,
    };
    assert!(matches!(
        uniform.walk("", json!({}), request, 10).steps[0],
        Step::Escalated {
            reason: Escalation::NoneOfThese,
            ..
        }
    ));
}

#[test]
fn paths_enumerates_routes_and_respects_avoid() {
    let cat = parse(TRIAGE).unwrap();
    let id = |n: &str| cat.object_id(n).unwrap();
    let shown = |ps: Vec<onto_core::Path>| ps.iter().map(|p| p.display(&cat)).collect::<Vec<_>>();
    assert_eq!(
        shown(cat.paths(id("Feature"), id("Done"), &[], 8)),
        ["build.plan", "implement.specify"]
    );
    assert_eq!(
        shown(cat.paths(id("Feature"), id("Done"), &[id("Spec")], 8)),
        ["build.plan"]
    );
    assert!(cat.paths(id("Bug"), id("Feature"), &[], 8).is_empty());
    assert_eq!(
        shown(cat.paths(id("Request"), id("Done"), &[], 2)),
        ["answer.ask", "wontfix.report"]
    );
}

const RICH: &str = r#"
# Meaning, primitives, levels and preconditions.
category Rich {
    objects: Case, Money, Help, Impact, Calm, Busy, Down, Consented, Marketing;
    about Money: "anything about what the customer paid"; # a comment; with ; and # inside strings above
    frame Case: choice {"question": "Which team?", "note": "semicolons; and #hashes stay inside"};
    pay:  Case -> Money "charges; refunds";
    help: Case -> Help  {"meaning": "how-to", "examples": ["export", "settings"]};
    closed: Case;

    frame Impact: score "How badly are users affected?";
    calm: Impact -> Calm level 0 "no impact";
    busy: Impact -> Busy level 1 "degraded";
    down: Impact -> Down level 2 "blocking";
    closed: Impact;

    market: Consented -> Marketing "send offers" require consent.marketing == true and age >= 16;
}
"#;

#[test]
fn parses_meaning_primitives_levels_and_require() {
    let cat = parse(RICH).unwrap();
    let case = cat.object(cat.object_id("Case").unwrap());
    assert_eq!(case.frame.primitive, Primitive::Choice);
    assert_eq!(
        case.frame.instructions.as_ref().unwrap()["note"],
        "semicolons; and #hashes stay inside"
    );
    assert_eq!(
        cat.object(cat.object_id("Money").unwrap()).about,
        Some(json!("anything about what the customer paid"))
    );
    assert_eq!(
        cat.arrow(cat.arrow_id("pay").unwrap()).instructions,
        Some(json!("charges; refunds"))
    );
    assert_eq!(
        cat.arrow(cat.arrow_id("help").unwrap())
            .instructions
            .as_ref()
            .unwrap()["examples"][0],
        "export"
    );
    assert_eq!(cat.arrow(cat.arrow_id("down").unwrap()).level, Some(2));
    assert_eq!(
        cat.object(cat.object_id("Impact").unwrap()).frame.primitive,
        Primitive::Score
    );
}

#[test]
fn require_filters_the_frame() {
    let cat = parse(RICH).unwrap();
    let consented = cat.object_id("Consented").unwrap();
    let n = |state| cat.eligible(consented, &state).len();
    assert_eq!(n(json!({"consent": {"marketing": true}, "age": 30})), 1);
    assert_eq!(n(json!({"consent": {"marketing": true}, "age": 12})), 0);
    assert_eq!(n(json!({"consent": {"marketing": false}, "age": 30})), 0);
    assert_eq!(
        n(json!({"age": 30})),
        0,
        "missing evidence never opens an arrow"
    );
}

#[test]
fn score_frames_need_distinct_levels() {
    let missing = "category C { objects: A, B; frame A: score; f: A -> B; }";
    assert!(matches!(parse(missing), Err(Error::Frame { .. })));
    let dup =
        "category C { objects: A, B, D; frame A: score; f: A -> B level 0; g: A -> D level 0; }";
    assert!(parse(dup).is_err());
    let stray = "category C { objects: A, B; f: A -> B level 1; }";
    assert!(parse(stray).is_err());
}

#[test]
fn noul_forks_only_when_the_model_and_the_budget_agree() {
    let holds = Answer::Noul {
        holds: vec![0.9, 0.2, 0.8],
        fork: Some(0.8),
    };
    assert!(matches!(
        decide(Closure::Closed, Some(&holds), 0.6, true),
        Decision::Fork { ref branches, .. } if branches.iter().map(|b| b.0).collect::<Vec<_>>() == [0, 2]
    ));
    // Budget exhausted: follow the best, keep the other as an alternative.
    assert_eq!(
        decide(Closure::Closed, Some(&holds), 0.6, false),
        Decision::Follow {
            index: 0,
            p: 0.9,
            alternatives: vec![(2, 0.8)]
        }
    );
    // Competing readings: the model says do not fork.
    let competing = Answer::Noul {
        holds: vec![0.9, 0.8],
        fork: Some(0.1),
    };
    assert!(matches!(
        decide(Closure::Closed, Some(&competing), 0.6, true),
        Decision::Follow { index: 0, .. }
    ));
    // Nothing holds; something is ambiguous.
    let unsure = Answer::Noul {
        holds: vec![0.5, 0.1],
        fork: None,
    };
    assert_eq!(
        decide(Closure::Closed, Some(&unsure), 0.6, true),
        Decision::Escalate(Escalation::LowConfidence)
    );
    let none = Answer::Noul {
        holds: vec![0.1, 0.05],
        fork: None,
    };
    assert_eq!(
        decide(Closure::Closed, Some(&none), 0.6, true),
        Decision::Escalate(Escalation::NoneOfThese)
    );
}

#[test]
fn score_follows_the_level_reached() {
    let sure = Answer::Score {
        levels: vec![0.0, 0.1, 0.9],
        confidence: Some(0.85),
    };
    assert!(matches!(
        decide(Closure::Closed, Some(&sure), 0.6, true),
        Decision::Follow { index: 2, .. }
    ));
    let torn = Answer::Score {
        levels: vec![0.0, 0.55, 0.45],
        confidence: Some(0.3),
    };
    assert_eq!(
        decide(Closure::Closed, Some(&torn), 0.6, true),
        Decision::Escalate(Escalation::LowConfidence)
    );
}

#[test]
fn open_frames_are_judged_and_name_their_gap() {
    let answer = |fit: f32| {
        Answer::Choice(Distribution {
            arrows: vec![fit],
            none_of_these: 1.0 - fit,
            confidence: Some(0.9),
        })
    };
    let (fits, nothing) = (answer(0.9), answer(0.1));
    assert!(matches!(
        decide(Closure::Open, Some(&fits), 0.6, true),
        Decision::Follow { index: 0, .. }
    ));
    let gap = |c, a| decide(c, a, 0.6, true);
    assert_eq!(
        gap(Closure::Open, Some(&nothing)),
        Decision::Escalate(Escalation::OpenFrame)
    );
    assert_eq!(
        gap(Closure::Closed, Some(&nothing)),
        Decision::Escalate(Escalation::NoneOfThese)
    );
    assert_eq!(
        gap(Closure::Open, None),
        Decision::Escalate(Escalation::OpenFrame)
    );
    assert_eq!(
        gap(Closure::Closed, None),
        Decision::Escalate(Escalation::NoneOfThese)
    );
}

mod supervisor {
    use super::*;
    use onto_core::supervise::{Admission, Outcome, admission, structural};
    use onto_core::walk::Proposal;

    const CONSENT: &str = r#"
    category Consent {
        objects: Collected, Consented, Contract, Marketing, Research;
        consent:  Collected -> Consented;
        contract: Collected -> Contract;
        market:   Consented -> Marketing;
        study:    Consented -> Research;
        invariant via: Collected -> Marketing through Consented;
        invariant never: Collected -> Advertiser;
        invariant rule "no new legal basis beyond consent and contract";
    }"#;

    fn proposal(arrow: &str, src: &str, dst: &str) -> Proposal {
        Proposal {
            arrow: arrow.into(),
            src: src.into(),
            dst: dst.into(),
            about: String::new(),
            rationale: String::new(),
        }
    }

    #[test]
    fn invariants_parse_and_hold_on_load() {
        let cat = parse(CONSENT).unwrap();
        assert_eq!(cat.invariants().len(), 3);
        let broken = CONSENT.replace(
            "study:    Consented -> Research;",
            "study: Consented -> Research; direct: Contract -> Marketing;",
        );
        let err = parse(&broken).unwrap_err();
        assert!(
            matches!(err, Error::InvariantViolated { ref witness, .. } if witness.starts_with("direct.contract")),
            "{err}"
        );
    }

    #[test]
    fn a_consent_bypass_is_rejected_with_a_counter_path() {
        let cat = parse(CONSENT).unwrap();
        let (checks, _) = structural(
            &cat,
            &proposal("legitimate_interest", "Collected", "Marketing"),
        );
        assert_eq!(admission(&checks), Admission::Reject);
        let failed = checks.iter().find(|c| c.outcome == Outcome::Fail).unwrap();
        assert_eq!(
            failed.subject,
            "via: Collected -> Marketing through Consented"
        );
        assert_eq!(failed.witness.as_deref(), Some("legitimate_interest"));
    }

    #[test]
    fn a_new_object_can_break_a_never_invariant() {
        let cat = parse(CONSENT).unwrap();
        let (checks, _) = structural(&cat, &proposal("sell", "Contract", "Advertiser"));
        let failed = checks
            .iter()
            .find(|c| c.outcome == Outcome::Fail)
            .expect("never: Collected -> Advertiser");
        assert_eq!(failed.witness.as_deref(), Some("sell.contract"));
    }

    #[test]
    fn a_safe_proposal_passes_structural_checks() {
        let cat = parse(CONSENT).unwrap();
        let (checks, extended) =
            structural(&cat, &proposal("newsletter", "Consented", "Newsletter"));
        assert_eq!(admission(&checks), Admission::Admit, "{checks:?}");
        assert!(extended.unwrap().object_id("Newsletter").is_ok());
        assert!(checks[0].reason.contains("new object Newsletter"));
    }

    #[test]
    fn malformed_proposals_fail_well_formedness() {
        let cat = parse(CONSENT).unwrap();
        for (arrow, src) in [
            ("market", "Consented"),
            ("x", "Nowhere"),
            ("id", "Collected"),
        ] {
            let (checks, extended) = structural(&cat, &proposal(arrow, src, "Somewhere"));
            assert!(extended.is_none(), "{arrow}");
            assert_eq!(
                (checks[0].check.as_str(), checks[0].outcome),
                ("well_formed", Outcome::Fail)
            );
        }
    }

    #[test]
    fn a_reused_name_does_not_hide_a_bypass() {
        let cat = parse(CONSENT).unwrap();
        // `market` already exists (Consented -> Marketing); reusing it for a
        // direct Collected -> Marketing arrow is malformed *and* a bypass.
        let (checks, extended) = structural(&cat, &proposal("market", "Collected", "Marketing"));
        assert!(extended.is_none());
        let failed: Vec<_> = checks
            .iter()
            .filter(|c| c.outcome == Outcome::Fail)
            .map(|c| c.check.as_str())
            .collect();
        assert_eq!(failed, ["well_formed", "invariant"]);
        let bypass = checks
            .iter()
            .find(|c| c.check == "invariant" && c.outcome == Outcome::Fail)
            .unwrap();
        assert_eq!(bypass.witness.as_deref(), Some("market"));
        assert!(!bypass.reason.contains("__proposed"), "{}", bypass.reason);
    }

    #[test]
    fn via_accepts_alternatives() {
        let src = CONSENT.replace(
            "invariant never: Collected -> Advertiser;",
            "invariant never: Collected -> Advertiser;\n        invariant via: Collected -> Research through Consented | Contract;",
        );
        let cat = parse(&src).unwrap();
        // Through Contract: allowed. Straight from Collected: a bypass.
        let (ok, _) = structural(&cat, &proposal("study_contract", "Contract", "Research"));
        assert_eq!(admission(&ok), Admission::Admit, "{ok:?}");
        let (bad, _) = structural(&cat, &proposal("skip_basis", "Collected", "Research"));
        let failed = bad.iter().find(|c| c.outcome == Outcome::Fail).unwrap();
        assert_eq!(
            failed.subject,
            "via: Collected -> Research through Consented | Contract"
        );
        assert_eq!(failed.witness.as_deref(), Some("skip_basis"));
    }

    #[test]
    fn reachable_returns_a_shortest_witness_or_none() {
        let cat = parse(CONSENT).unwrap();
        let id = |n| cat.object_id(n).unwrap();
        assert_eq!(
            cat.reachable(id("Collected"), id("Marketing"), &[])
                .unwrap()
                .display(&cat),
            "market.consent"
        );
        assert!(
            cat.reachable(id("Collected"), id("Marketing"), &[id("Consented")])
                .is_none()
        );
        assert!(
            cat.reachable(id("Marketing"), id("Collected"), &[])
                .is_none()
        );
    }
}
