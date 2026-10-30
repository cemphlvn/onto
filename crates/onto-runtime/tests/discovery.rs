//! Discovery asks the judge with at most `concurrency` calls in flight,
//! and a failed call fails only its own object.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use onto_core::walk::Answer;
use onto_core::{Category, ObjId};
use onto_runtime::model::{FrameRequest, Judge, MockJudge, ModelError, Usage};

/// The mock judge, counting how many of its calls overlap.
struct Counting {
    inner: MockJudge,
    now: AtomicUsize,
    most: AtomicUsize,
}

impl Judge for Counting {
    fn name(&self) -> String {
        "counting".into()
    }
    async fn judge(&self, req: FrameRequest) -> Result<(Answer, Usage), ModelError> {
        let n = self.now.fetch_add(1, Ordering::SeqCst) + 1;
        self.most.fetch_max(n, Ordering::SeqCst);
        let out = self.inner.judge(req).await;
        self.now.fetch_sub(1, Ordering::SeqCst);
        out
    }
}

/// The mock judge, overloaded for one source object.
struct Overloaded {
    inner: MockJudge,
    at: &'static str,
}

impl Judge for Overloaded {
    fn name(&self) -> String {
        "overloaded".into()
    }
    async fn judge(&self, req: FrameRequest) -> Result<(Answer, Usage), ModelError> {
        if req.at == self.at {
            return Err(ModelError::Status {
                status: 529,
                body: "system_overloaded".into(),
            });
        }
        self.inner.judge(req).await
    }
}

fn cats() -> (Category, Category) {
    let a = onto_core::parse(
        "category A { objects: P1, P2, P3, P4, P5, P6, P7, P8; \
         d1: P1 -> P2; d2: P3 -> P4; d3: P5 -> P6; d4: P7 -> P8; }",
    )
    .unwrap();
    let b = onto_core::parse("category B { objects: X, Y, Z; xy: X -> Y; yz: Y -> Z; }").unwrap();
    (a, b)
}

#[tokio::test]
async fn discovery_keeps_judge_calls_within_the_limit() {
    let (a, b) = cats();
    let adm = onto_core::discover::unconstrained(&a, &b);
    let judge = Counting {
        inner: MockJudge {
            latency: Duration::from_millis(20),
        },
        now: AtomicUsize::new(0),
        most: AtomicUsize::new(0),
    };
    let out =
        onto_runtime::discovery::judge_objects(&judge, &a, &b, &adm, 2, &Default::default()).await;
    assert!(out.failed.is_empty());
    assert!(
        out.judged.iter().all(Option::is_some),
        "every object was judged"
    );
    let most = judge.most.load(Ordering::SeqCst);
    assert!(
        (1..=2).contains(&most),
        "at most 2 calls in flight, saw {most}"
    );

    // The limit is what bounds it: with room for all, calls overlap.
    judge.most.store(0, Ordering::SeqCst);
    onto_runtime::discovery::judge_objects(&judge, &a, &b, &adm, 8, &Default::default()).await;
    assert!(judge.most.load(Ordering::SeqCst) > 2);
}

#[tokio::test]
async fn a_failed_call_fails_only_its_object() {
    let (a, b) = cats();
    let adm = onto_core::discover::unconstrained(&a, &b);
    let judge = Overloaded {
        inner: MockJudge {
            latency: Duration::from_millis(1),
        },
        at: "P3",
    };
    let out =
        onto_runtime::discovery::judge_objects(&judge, &a, &b, &adm, 4, &Default::default()).await;
    let p3 = a.object_id("P3").unwrap();
    assert_eq!(out.asked, 8);
    assert_eq!(out.failed.len(), 1);
    assert_eq!(out.failed[0].0, p3);
    assert!(matches!(
        out.failed[0].1,
        ModelError::Status { status: 529, .. }
    ));
    for (i, j) in out.judged.iter().enumerate() {
        assert_eq!(j.is_none(), ObjId(i as u32) == p3, "object {i}");
    }
}
