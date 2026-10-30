//! Functor discovery by meaning: for each source object, a judge chooses
//! among the target objects its structural role allows (or none: the
//! functor stays partial). Structure narrows the question first
//! (`onto_core::discover::admissible`), so the judge selects, never
//! invents. The scores feed `onto_core::discover::search`.

use onto_core::{Category, ObjId, Primitive};
use serde_json::{Value, json};

use crate::model::{Candidate, FrameRequest, Judge, ModelError};
use onto_core::walk::Answer;

/// What an object means in its category: its description, the question
/// asked there, the options it offers, and how it is reached.
pub fn describe(cat: &Category, o: ObjId) -> Value {
    let obj = cat.object(o);
    let text = |v: &Option<Value>| {
        v.as_ref()
            .map(|v| v.as_str().map_or(v.to_string(), str::to_owned))
    };
    let arrow = |a: onto_core::ArrowId| {
        let x = cat.arrow(a);
        json!({
            "arrow": x.name,
            "means": text(&x.instructions),
        })
    };
    let reached: Vec<Value> = cat
        .arrows()
        .iter()
        .enumerate()
        .filter(|(_, a)| a.dst == o)
        .map(|(i, _)| arrow(onto_core::ArrowId(i as u32)))
        .collect();
    json!({
        "object": obj.name,
        "category": cat.name(),
        "about": text(&obj.about),
        "question": text(&obj.frame.instructions),
        "reached_by": reached,
        "options": cat.out(o).iter().map(|a| arrow(*a)).collect::<Vec<_>>(),
    })
}

/// One source object's judgment: a probability per admissible target,
/// and the probability that none corresponds.
#[derive(Clone, Debug)]
pub struct Judged {
    pub targets: Vec<(ObjId, f32)>,
    pub none: f32,
    pub confidence: Option<f32>,
    /// The call's tokens and attempts, for cost.
    pub usage: crate::model::Usage,
}

/// Every source object's judgment, and the objects whose judge call
/// failed. A failure stays with its object: the others are kept, and the
/// caller decides how many failures a discovery may absorb.
#[derive(Debug, Default)]
pub struct Judgments {
    /// Per source object; `None` when structure settled it, when it has
    /// no admissible target, or when its call failed.
    pub judged: Vec<Option<Judged>>,
    /// Objects asked whose call failed (after the client's retries).
    pub failed: Vec<(ObjId, ModelError)>,
    /// Objects asked.
    pub asked: usize,
}

/// What the judge is told the correspondence is for: a goal and the
/// question's instructions, templates over `{src}`, `{dst}` and
/// `{object}` (the source category, the target category, the object
/// judged). The default reads two organisations' case handling; a
/// mapping from code to an architecture needs its own.
#[derive(Clone, Debug)]
pub struct Framing {
    pub goal: String,
    pub instructions: String,
}

impl Default for Framing {
    fn default() -> Self {
        Self {
            goal: "Two organisations are merging. Find the object of {dst} that plays the same role as {object} of {src} when a case is handled.".into(),
            instructions: "Which object of {dst} corresponds to the source object: the same kind of case, the same step in handling it? Judge by meaning (descriptions, the options it offers, how it is reached), not by name. If none of them does, answer none of these.".into(),
        }
    }
}

impl Framing {
    fn fill(template: &str, a: &Category, b: &Category, x: ObjId) -> String {
        template
            .replace("{src}", a.name())
            .replace("{dst}", b.name())
            .replace("{object}", &a.object(x).name)
    }
}

/// Asks the judge, for every source object with more than one admissible
/// target, which one corresponds. Objects with exactly one admissible
/// target are settled by structure (`None` here); objects with none are
/// outside the domain. At most `concurrency` calls are in flight, as in
/// the engine (`Config::judge_concurrency`): one discovery must not
/// overload the judge it depends on. A failed call fails only its object
/// ([`Judgments::failed`]); it never ends the discovery.
pub async fn judge_objects<J: Judge>(
    judge: &J,
    a: &Category,
    b: &Category,
    admissible: &[Vec<ObjId>],
    concurrency: usize,
    framing: &Framing,
) -> Judgments {
    let slots = &tokio::sync::Semaphore::new(concurrency.max(1));
    let asks: Vec<usize> = (0..admissible.len())
        .filter(|i| admissible[*i].len() > 1)
        .collect();
    let futs: Vec<_> = asks
        .iter()
        .map(|&i| {
            let x = ObjId(i as u32);
            let targets = admissible[i].clone();
            let req = FrameRequest {
                state: json!({
                    "goal": Framing::fill(&framing.goal, a, b, x),
                    "source": describe(a, x),
                }),
                at: a.object(x).name.clone(),
                focus: None,
                primitive: Primitive::Choice,
                instructions: Some(Value::String(Framing::fill(&framing.instructions, a, b, x))),
                candidates: targets
                    .iter()
                    .map(|y| Candidate {
                        arrow: b.object(*y).name.clone(),
                        to: b.object(*y).name.clone(),
                        instructions: Some(describe(b, *y)),
                        level: None,
                    })
                    .collect(),
                can_fork: false,
                parallel: false,
            };
            Box::pin(async move {
                let _slot = slots.acquire().await.expect("semaphore never closed");
                let (answer, usage) = judge.judge(req).await?;
                let Answer::Choice(d) = answer else {
                    return Err(ModelError::Decode("expected a choice".into()));
                };
                Ok::<_, ModelError>(Judged {
                    targets: targets.into_iter().zip(d.arrows).collect(),
                    none: d.none_of_these,
                    confidence: d.confidence,
                    usage,
                })
            })
        })
        .collect();
    let answers = crate::ensemble::join_all(futs).await;
    let mut out = Judgments {
        judged: vec![None; admissible.len()],
        failed: Vec::new(),
        asked: asks.len(),
    };
    for (i, r) in asks.into_iter().zip(answers) {
        match r {
            Ok(j) => out.judged[i] = Some(j),
            Err(e) => out.failed.push((ObjId(i as u32), e)),
        }
    }
    out
}
