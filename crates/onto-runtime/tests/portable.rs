//! The same runs on both executors (D70): tokio natively, a JavaScript
//! event loop on wasm32 (`wasm-bindgen-test`, in Node). The paths must be
//! the same; only the executor differs.
//!
//!     cargo test -p onto-runtime --test portable
//!     cargo test -p onto-runtime --test portable --target wasm32-unknown-unknown

use std::sync::Arc;
use std::time::Duration;

use onto_runtime::model::{MockJudge, MockProposer};
use onto_runtime::{Config, Engine, Job};
use serde_json::{Value, json};

const TRIAGE: &str = include_str!("../../../examples/triage.onto");
const SUPPORT: &str = include_str!("../../../demos/support-commons/large.onto");
const SUPPORT_JOBS: &str = include_str!("../../../demos/support-commons/large.jobs");

fn jobs(text: &str) -> Vec<Job> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| {
            let (from, rest) = l.split_once(':').unwrap();
            let case: Value = serde_json::from_str(rest.trim()).unwrap();
            Job {
                from: from.trim().into(),
                goal: case["goal"].as_str().unwrap().into(),
                case,
            }
        })
        .collect()
}

async fn paths(cat: onto_core::Category, jobs: Vec<Job>) -> Vec<String> {
    let latency = Duration::from_millis(5);
    let cfg = Config {
        open_world: false,
        ..Config::default()
    };
    let engine = Engine::new(
        Arc::new(cat),
        MockJudge { latency },
        MockProposer { latency },
        cfg,
    );
    let report = engine.run(jobs).await.unwrap();
    report
        .walks
        .iter()
        .filter(|w| w.parent.is_none())
        .map(|w| w.path.clone())
        .collect()
}

async fn triage() {
    let cat = onto_core::parse(TRIAGE).unwrap();
    let fix = Job {
        from: "Request".into(),
        goal: "a bug: patch it and ship".into(),
        case: json!({}),
    };
    let got = paths(cat, vec![fix.clone(), fix.clone(), fix]).await;
    assert_eq!(got, vec!["ship.patch.report : Request -> Done"; 3]);
}

/// The 22 labelled support tickets: every one walks to the same place on
/// either executor (the native run's paths, recorded below).
async fn support() {
    let module = onto_core::parse_module(SUPPORT, &mut |p| {
        Err(onto_core::Error::Parse {
            line: 0,
            msg: format!("no imports here: {p}"),
        })
    })
    .unwrap();
    let cat = module.categories.into_iter().next().unwrap();
    let got = paths(cat, jobs(SUPPORT_JOBS)).await;
    assert_eq!(got.len(), 22);
    assert_eq!(got, SUPPORT_PATHS);
}

/// Recorded from the native run (the offline mock judge; `id(Ticket)` is a
/// ticket the mock could not place, escalated at the entry frame).
const SUPPORT_PATHS: [&str; 22] = [
    "refund : Ticket -> Refund",
    "refund : Ticket -> Refund",
    "invoice_request : Ticket -> InvoiceRequest",
    "payment_failed : Ticket -> PaymentFailed",
    "id(Ticket) : Ticket -> Ticket",
    "error_message : Ticket -> ErrorMessage",
    "app_crash : Ticket -> AppCrash",
    "cancel_subscription : Ticket -> CancelSubscription",
    "login_problem : Ticket -> LoginProblem",
    "two_factor : Ticket -> TwoFactor",
    "delete_account : Ticket -> DeleteAccount",
    "double_charge : Ticket -> DoubleCharge",
    "where_is_order : Ticket -> WhereIsOrder",
    "where_is_order : Ticket -> WhereIsOrder",
    "id(Ticket) : Ticket -> Ticket",
    "password_reset : Ticket -> PasswordReset",
    "login_problem : Ticket -> LoginProblem",
    "pricing_question : Ticket -> PricingQuestion",
    "where_is_order : Ticket -> WhereIsOrder",
    "integration_broken : Ticket -> IntegrationBroken",
    "error_message : Ticket -> ErrorMessage",
    "pricing_question : Ticket -> PricingQuestion",
];

#[cfg(not(target_arch = "wasm32"))]
mod native {
    #[tokio::test(flavor = "multi_thread")]
    async fn triage() {
        super::triage().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn support() {
        super::support().await;
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test]
    async fn triage() {
        super::triage().await;
    }

    #[wasm_bindgen_test]
    async fn support() {
        super::support().await;
    }
}
