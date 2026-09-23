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
    let n = |state| cat.eligible(consented, &state, &Default::default()).len();
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
            ..Default::default()
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

mod contracts {
    use super::*;
    use onto_core::Gate;
    use onto_core::laws::{derive, enforced};
    use onto_core::supervise::{Admission, Outcome, admission, structural};
    use onto_core::walk::Proposal;
    use std::collections::BTreeSet;

    /// Consent is a checked fact (require on the case) that yields a
    /// capability (ensures ConsentGrant); Marketing needs the capability on
    /// entry; withdrawing consent revokes it.
    const SRC: &str = r#"
    category C {
        objects: Collected, Consented, Pseudonymized, Research, Marketing;
        consent:  Collected -> Consented require consent.given == true ensures ConsentGrant, Basis;
        contract: Collected -> Pseudonymized require contract == true ensures Basis;
        pseudo:   Consented -> Pseudonymized;
        study:    Pseudonymized -> Research;
        market:   Consented -> Marketing;
        withdraw: Consented -> Collected revokes ConsentGrant, Basis;
        entry Marketing: needs ConsentGrant require consent.marketing == true;
        entry Pseudonymized: needs Basis;
        capability ConsentGrant { issuers: consent; revokers: withdraw; }
        capability Basis { issuers: consent, contract; revokers: withdraw; }
        invariant via: Collected -> Marketing through Consented;
    }"#;

    fn tokens(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn contracts_parse_and_gate_arrows() {
        let cat = parse(SRC).unwrap();
        let id = |n| cat.arrow_id(n).unwrap();
        assert_eq!(cat.arrow(id("consent")).ensures, ["ConsentGrant", "Basis"]);
        assert_eq!(cat.arrow(id("withdraw")).revokes, ["ConsentGrant", "Basis"]);
        let case = json!({"consent": {"given": true, "marketing": true}});
        // Marketing needs a token the walk does not hold yet.
        assert!(
            matches!(cat.gate(id("market"), &case, &tokens(&[])), Gate::Entry { ref missing, .. } if missing == &["ConsentGrant"])
        );
        assert_eq!(
            cat.gate(id("market"), &case, &tokens(&["ConsentGrant"])),
            Gate::Open
        );
        // Entry case precondition still applies.
        let no_opt_in = json!({"consent": {"given": true, "marketing": false}});
        assert!(matches!(
            cat.gate(id("market"), &no_opt_in, &tokens(&["ConsentGrant"])),
            Gate::Entry {
                require_failed: true,
                ..
            }
        ));
        // A token is only minted where the evidence holds.
        assert_eq!(
            cat.gate(id("consent"), &json!({}), &tokens(&[])),
            Gate::Require
        );
    }

    #[test]
    fn walks_carry_and_apply_tokens() {
        let cat = parse(SRC).unwrap();
        let mut w = Walker {
            cat: &cat,
            judge: ScriptedJudge::new(["consent", "market"]),
            proposer: NullProposer,
            threshold: 0.5,
        };
        let case = json!({"consent": {"given": true, "marketing": true}});
        let walk = w.walk("offers", case, cat.object_id("Collected").unwrap(), 5);
        assert_eq!(walk.state.path.display(&cat), "market.consent");
        assert_eq!(walk.state.tokens, tokens(&["Basis", "ConsentGrant"]));
    }

    #[test]
    fn laws_drop_out_of_local_contracts() {
        let cat = parse(SRC).unwrap();
        let laws = derive(&cat, &[]);
        let o = |n| cat.object_id(n).unwrap();
        // Walks may start wherever entering needs no tokens.
        assert_eq!(laws.starts, [o("Collected"), o("Consented"), o("Research")]);
        assert_eq!(
            laws.must(o("Marketing")),
            Some(tokens(&["Basis", "ConsentGrant"]))
        );
        // Research states no contract, yet every walk into it holds Basis:
        // derived from Pseudonymized's entry contract.
        assert!(cat.object(o("Research")).entry.is_empty());
        assert!(laws.must(o("Research")).unwrap().contains("Basis"));
        // Returning to Collected by withdraw clears the tokens.
        assert_eq!(laws.must(o("Collected")), Some(tokens(&[])));
        assert!(laws.dead(&cat).is_empty());
    }

    #[test]
    fn contracts_enforce_invariants_for_walks_even_where_the_graph_does_not() {
        let with_bypass = SRC.replace(
            "invariant via: Collected -> Marketing through Consented;",
            "ads: Collected -> Marketing;",
        );
        let cat = parse(&with_bypass).unwrap();
        // The bare graph has a bypass (`ads`), but no walk can take it:
        // Marketing needs ConsentGrant, and only `consent` grants it.
        assert!(
            cat.reachable(
                cat.object_id("Collected").unwrap(),
                cat.object_id("Marketing").unwrap(),
                &[cat.object_id("Consented").unwrap()]
            )
            .is_some()
        );
        let inv = onto_core::Invariant::Via {
            from: "Collected".into(),
            to: "Marketing".into(),
            through: vec!["Consented".into()],
        };
        assert_eq!(enforced(&cat, &inv), Some(true));
        let dead: Vec<_> = derive(&cat, &[])
            .dead(&cat)
            .iter()
            .map(|a| cat.arrow(*a).name.clone())
            .collect();
        assert_eq!(dead, ["ads"]);
    }

    #[test]
    fn review_notes_dead_proposals_and_challenges_closed_frames() {
        let cat = parse(&SRC.replace(
            "invariant via: Collected -> Marketing through Consented;",
            "closed: Consented;",
        ))
        .unwrap();
        let p = |arrow: &str, src: &str, dst: &str| Proposal {
            arrow: arrow.into(),
            src: src.into(),
            dst: dst.into(),
            about: String::new(),
            rationale: String::new(),
            ..Default::default()
        };

        let (checks, _) = structural(&cat, &p("ads", "Collected", "Marketing"));
        let note = checks
            .iter()
            .find(|c| c.check == "contract")
            .expect("contract note");
        assert!(
            note.reason.contains("needs ConsentGrant"),
            "{}",
            note.reason
        );

        let (checks, _) = structural(&cat, &p("newsletter", "Consented", "Newsletter"));
        let challenge = checks
            .iter()
            .find(|c| c.check == "closure")
            .expect("closure challenge");
        assert_eq!(challenge.outcome, Outcome::Unknown);
        assert_eq!(admission(&checks), Admission::Unknown);
    }
}

mod authority {
    use super::*;
    use onto_core::laws::{Proof, derive};
    use onto_core::supervise::{Admission, Outcome, admission, structural};
    use onto_core::walk::Proposal;

    // Declarations come first, before the arrows they name: order must not matter.
    const SRC: &str = r#"
    category C {
        capability LegalBasis { issuers: consent, contract; revokers: withdraw; }
        objects: Collected, Consented, Contract, Pseudonymized, Research;
        start: Collected;
        consent:  Collected -> Consented require consent.given == true ensures LegalBasis;
        contract: Collected -> Contract require contract.active == true ensures LegalBasis;
        pseudo:   Consented -> Pseudonymized;
        fulfil:   Contract -> Pseudonymized;
        study:    Pseudonymized -> Research;
        withdraw: Consented -> Collected revokes LegalBasis;
        entry Pseudonymized: needs LegalBasis;
    }"#;

    fn with(extra: &str) -> Result<onto_core::Category, Error> {
        parse(&SRC.replace(
            "entry Pseudonymized: needs LegalBasis;",
            &format!("entry Pseudonymized: needs LegalBasis;\n{extra}"),
        ))
    }

    #[test]
    fn authorized_issuance_loads() {
        let cat = parse(SRC).unwrap();
        assert_eq!(
            cat.capability("LegalBasis").unwrap().issuers,
            ["consent", "contract"]
        );
        assert_eq!(cat.starts(), [cat.object_id("Collected").unwrap()]);
    }

    #[test]
    fn unauthorized_minting_fails_to_load() {
        let err = with("fake_basis: Collected -> Pseudonymized ensures LegalBasis;").unwrap_err();
        assert_eq!(
            err.to_string(),
            "capability: unauthorized issuer of LegalBasis: `fake_basis` (authorized: consent, contract)"
        );
    }

    #[test]
    fn unauthorized_revocation_and_bad_declarations_fail() {
        assert!(
            with("forget: Consented -> Collected revokes LegalBasis;")
                .unwrap_err()
                .to_string()
                .contains("unauthorized revoker")
        );
        assert!(
            with("capability LegalBasis { issuers: consent; }")
                .unwrap_err()
                .to_string()
                .contains("declared twice")
        );
        assert!(
            with("capability Other { issuers: nowhere; }")
                .unwrap_err()
                .to_string()
                .contains("unknown arrow `nowhere`")
        );
        assert!(
            with("x: Collected -> Research ensures Undeclared;")
                .unwrap_err()
                .to_string()
                .contains("undeclared capability Undeclared")
        );
        assert!(
            with("start: Pseudonymized;")
                .unwrap_err()
                .to_string()
                .contains("cannot start there")
        );
    }

    #[test]
    fn the_supervisor_uses_the_same_validator() {
        let cat = parse(SRC).unwrap();
        let fake = Proposal {
            arrow: "fake_basis".into(),
            src: "Collected".into(),
            dst: "Pseudonymized".into(),
            ensures: vec!["LegalBasis".into()],
            ..Default::default()
        };
        let (checks, _) = structural(&cat, &fake);
        let cap = checks.iter().find(|c| c.check == "capability").unwrap();
        assert_eq!(cap.outcome, Outcome::Fail);
        assert!(
            cap.reason
                .contains("unauthorized issuer of LegalBasis: `fake_basis`"),
            "{}",
            cap.reason
        );
        assert_eq!(admission(&checks), Admission::Reject);
    }

    #[test]
    fn laws_carry_certificates_and_counterexamples() {
        let cat = parse(SRC).unwrap();
        let laws = derive(&cat, cat.starts());
        let o = |n| cat.object_id(n).unwrap();
        match laws.prove(&cat, o("Research"), "LegalBasis") {
            Some(Proof::Must {
                arrival_states,
                granted_by,
                revoked_by,
            }) => {
                assert!(arrival_states >= 1);
                let names: Vec<_> = granted_by
                    .iter()
                    .map(|a| cat.arrow(*a).name.as_str())
                    .collect();
                assert_eq!(names, ["consent", "contract"]);
                assert_eq!(
                    revoked_by
                        .iter()
                        .map(|a| cat.arrow(*a).name.as_str())
                        .collect::<Vec<_>>(),
                    ["withdraw"]
                );
            }
            other => panic!("{other:?}"),
        }
        // Collected is re-entered by `withdraw`, without LegalBasis.
        let w = laws.path_to(o("Collected"), &Default::default()).unwrap();
        assert_eq!(
            w.iter()
                .map(|a| cat.arrow(*a).name.as_str())
                .collect::<Vec<_>>(),
            ["consent", "withdraw"]
        );
        assert!(laws.witness(o("Research"), "LegalBasis").is_some());
    }
}

mod joins {
    use super::*;
    use onto_core::Join;

    const SRC: &str = r#"category C {
        capability Checked { issuers: verify; }
        objects: Alert, Latency, Security, Merge, Done;
        frame Alert: noul;
        latency:  Alert -> Latency;
        security: Alert -> Security;
        look:   Latency -> Merge;
        verify: Security -> Merge ensures Checked;
        finish: Merge -> Done;
        JOIN
    }"#;

    #[test]
    fn join_declarations_parse() {
        let cat = parse(&SRC.replace(
            "JOIN",
            "join Merge: gate authority security export Checked;",
        ))
        .unwrap();
        assert_eq!(
            cat.object(cat.object_id("Merge").unwrap()).join,
            Some(Join::Gate {
                authority: "security".into(),
                export: vec!["Checked".into()]
            })
        );
        for j in ["all", "race"] {
            assert!(parse(&SRC.replace("JOIN", &format!("join Merge: {j};"))).is_ok());
        }
    }

    #[test]
    fn gate_authority_and_exports_are_validated() {
        let err = |j: &str| parse(&SRC.replace("JOIN", j)).unwrap_err().to_string();
        assert!(
            err("join Merge: gate authority look;").contains("must leave a noul or split frame")
        );
        assert!(err("join Merge: gate authority nope;").contains("unknown authority arrow"));
        assert!(
            err("join Merge: gate authority security export Forged;")
                .contains("undeclared capability Forged")
        );
        assert!(err("join Merge: maybe;").contains("unknown join"));
    }
}

#[test]
fn categories_know_their_snapshot() {
    let cat = parse(TRIAGE).unwrap();
    let hash = onto_core::parse::snapshot_hash(TRIAGE);
    assert_eq!(cat.snapshot(), Some(hash.as_str()));
    assert_eq!(hash.len(), 64);
    // A different source is a different snapshot; a hypothetical
    // extension has none (it is not a file anyone can point to).
    assert_ne!(
        onto_core::parse::snapshot_hash(&format!("{TRIAGE}\n")),
        hash
    );
    let ext = cat
        .extend("x", "Request", "Somewhere", Default::default())
        .unwrap();
    assert_eq!(ext.snapshot(), None);
}

mod attestation {
    use super::*;
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD as B64;
    use ed25519_dalek::{Signer, SigningKey};
    use onto_core::Gate;
    use onto_core::attest::{attested_view, message};
    use serde_json::Value;

    fn key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn public(k: &SigningKey) -> String {
        format!("ed25519:{}", B64.encode(k.verifying_key().to_bytes()))
    }

    fn src() -> String {
        format!(
            r#"category C {{
            attester DeployBot {{ key: {}; observes: errors.stopped; }}
            objects: Rollback, Mitigated;
            verify: Rollback -> Mitigated "confirm errors stopped" attested errors.stopped == true;
        }}"#,
            public(&key(1))
        )
    }

    fn observation(signer: &SigningKey, attester: &str, case: &str, field: &str) -> Value {
        let claim = json!({ field: true }).as_object().unwrap().clone();
        let at = "2026-09-23T10:02:00Z";
        let sig = signer.sign(&message(attester, case, at, &claim));
        json!({"claim": claim, "attester": attester, "case": case, "at": at, "signature": B64.encode(sig.to_bytes())})
    }

    fn gate(case: Value) -> Gate {
        let cat = parse(&src()).unwrap();
        cat.gate(cat.arrow_id("verify").unwrap(), &case, &Default::default())
    }

    #[test]
    fn a_valid_signed_observation_opens_the_arrow() {
        let case = json!({"id": "INC-1", "observations": [observation(&key(1), "DeployBot", "INC-1", "errors.stopped")]});
        assert_eq!(gate(case), Gate::Open);
    }

    #[test]
    fn nothing_else_opens_it() {
        let obs =
            |k: u8, att: &str, case: &str, field: &str| observation(&key(k), att, case, field);
        let cases = [
            ("no observation", json!({"id": "INC-1"})),
            (
                "a plain fact",
                json!({"id": "INC-1", "errors": {"stopped": true}}),
            ),
            (
                "a forged key",
                json!({"id": "INC-1", "observations": [obs(2, "DeployBot", "INC-1", "errors.stopped")]}),
            ),
            (
                "a replay",
                json!({"id": "INC-2", "observations": [obs(1, "DeployBot", "INC-1", "errors.stopped")]}),
            ),
            (
                "an undeclared attester",
                json!({"id": "INC-1", "observations": [obs(1, "SomeBot", "INC-1", "errors.stopped")]}),
            ),
            (
                "an unauthorized field",
                json!({"id": "INC-1", "observations": [obs(1, "DeployBot", "INC-1", "keys.revoked")]}),
            ),
        ];
        for (what, case) in cases {
            assert_eq!(
                gate(case),
                Gate::Unattested,
                "{what} must not open an attested arrow"
            );
        }
    }

    #[test]
    fn tampering_with_a_signed_claim_breaks_it() {
        let mut o = observation(&key(1), "DeployBot", "INC-1", "errors.stopped");
        o["at"] = json!("2027-01-01T00:00:00Z");
        let cat = parse(&src()).unwrap();
        let (_, ok, bad) = attested_view(
            cat.attesters(),
            &json!({"id": "INC-1", "observations": [o]}),
        );
        assert!(ok.is_empty());
        assert_eq!(bad[0].reason, "signature does not verify");
    }

    #[test]
    fn attesters_are_validated_as_policy() {
        let s = src();
        assert!(
            parse(&s.replace("observes: errors.stopped;", "observes: other;"))
                .unwrap_err()
                .to_string()
                .contains("no declared attester may observe it")
        );
        let twice = s.replace(
            "objects:",
            &format!(
                "attester DeployBot {{ key: {}; observes: x; }}\n objects:",
                public(&key(3))
            ),
        );
        assert!(
            parse(&twice)
                .unwrap_err()
                .to_string()
                .contains("declared twice")
        );
        assert!(parse(&s.replace("key: ed25519:", "key: rsa:")).is_err());
    }
}

mod quotient {
    use super::*;
    use onto_core::quotient::{Mode, label_only_identities, partition, redundant_arrows};

    fn names(cat: &onto_core::Category, classes: Vec<Vec<onto_core::ObjId>>) -> Vec<Vec<String>> {
        classes
            .into_iter()
            .map(|g| g.into_iter().map(|o| cat.object(o).name.clone()).collect())
            .collect()
    }

    #[test]
    fn refinement_separates_by_depth_and_merges_by_behaviour() {
        // A -> B -> C and D -> E: B and D each lead to a terminal, A does not.
        let cat = parse("category C { objects: A, B, C, D, E; ab: A -> B; bc: B -> C; de: D -> E; closed: A, B, C, D, E; }").unwrap();
        let classes = names(&cat, partition(&cat, Mode::Exact));
        assert_eq!(classes, [vec!["A"], vec!["B", "D"], vec!["C", "E"]]);
    }

    #[test]
    fn support_taxonomy_identities_rest_on_descriptions() {
        let src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../demos/support-commons/support.onto"
        ))
        .unwrap();
        let cat = parse(&src).unwrap();
        assert!(
            partition(&cat, Mode::Exact).iter().all(|g| g.len() == 1),
            "no genuine duplicates"
        );
        let label_only = names(&cat, label_only_identities(&cat));
        assert!(
            label_only.contains(
                &[
                    "Refund",
                    "Invoice",
                    "Outage",
                    "HowTo",
                    "Login",
                    "FeatureRequest"
                ]
                .map(String::from)
                .to_vec()
            ),
            "{label_only:?}"
        );
    }

    #[test]
    fn preconditions_and_effects_are_structure() {
        // Same shape, but one arrow needs evidence: not equivalent, even ignoring descriptions.
        let cat = parse("category C { objects: A, B, T; a: A -> T \"x\"; b: B -> T \"y\" require ok == true; closed: A, B, T; }").unwrap();
        assert_eq!(partition(&cat, Mode::Structure).len(), 3);
        // Without the precondition they collapse structurally, but not exactly.
        let cat = parse(
            "category C { objects: A, B, T; a: A -> T \"x\"; b: B -> T \"y\"; closed: A, B, T; }",
        )
        .unwrap();
        assert_eq!(partition(&cat, Mode::Structure).len(), 2);
        assert_eq!(partition(&cat, Mode::Exact).len(), 3);
    }

    #[test]
    fn redundant_arrows_are_exact_repeats() {
        let cat = parse("category C { objects: A, B; f: A -> B \"same\"; g: A -> B \"same\"; h: A -> B \"other\"; }").unwrap();
        let r: Vec<_> = redundant_arrows(&cat)
            .iter()
            .map(|(e, l)| (cat.arrow(*e).name.clone(), cat.arrow(*l).name.clone()))
            .collect();
        assert_eq!(r, [("f".to_owned(), "g".to_owned())]);
    }
}

#[test]
fn split_frames_parse_and_reject_levels() {
    let cat = parse("category C { objects: A, B; frame A: split; f: A -> B; }").unwrap();
    assert_eq!(
        cat.object(cat.object_id("A").unwrap()).frame.primitive,
        Primitive::Split
    );
    assert!(parse("category C { objects: A, B; frame A: split; f: A -> B level 0; }").is_err());
    assert!(parse("category C { objects: A, B; frame A: choice parallel; f: A -> B; }").is_err());
}
