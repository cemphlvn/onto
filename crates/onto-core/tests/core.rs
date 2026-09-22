use onto_core::walk::{Escalation, NullProposer, ScriptedChooser, Step, UniformChooser, Walker};
use onto_core::{Equality, Error, Verdict, category::resolve, parse, parse::path_spec};

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
        chooser: ScriptedChooser::new(["report", "patch", "ship"]),
        proposer: NullProposer,
        threshold: 0.5,
    };
    let walk = w.walk("login page crashes", cat.object_id("Request").unwrap(), 10);
    assert_eq!(walk.steps.len(), 3);
    assert_eq!(walk.state.path.display(&cat), "ship.patch.report");
}

#[test]
fn walk_escalates_on_open_frame() {
    let cat = parse(TRIAGE).unwrap();
    let mut w = Walker {
        cat: &cat,
        chooser: ScriptedChooser::new(["ask"]),
        proposer: NullProposer,
        threshold: 0.5,
    };
    let walk = w.walk("", cat.object_id("Request").unwrap(), 10);
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
        chooser: ScriptedChooser::new(["refund"]),
        proposer: NullProposer,
        threshold: 0.5,
    };
    assert!(matches!(
        scripted.walk("", request, 10).steps[0],
        Step::Escalated {
            reason: Escalation::NoneOfThese,
            ..
        }
    ));

    let mut uniform = Walker {
        cat: &cat,
        chooser: UniformChooser,
        proposer: NullProposer,
        threshold: 0.5,
    };
    assert!(matches!(
        uniform.walk("", request, 10).steps[0],
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
