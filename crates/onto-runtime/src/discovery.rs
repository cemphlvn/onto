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
}

/// Asks the judge, for every source object with more than one admissible
/// target, which one corresponds. Objects with exactly one admissible
/// target are settled by structure (`None` here); objects with none are
/// outside the domain.
pub async fn judge_objects<J: Judge>(
    judge: &J,
    a: &Category,
    b: &Category,
    admissible: &[Vec<ObjId>],
) -> Result<Vec<Option<Judged>>, ModelError> {
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
                    "goal": format!(
                        "Two organisations are merging. Find the object of {} that plays the same role as {} of {} when a case is handled.",
                        b.name(), a.object(x).name, a.name()
                    ),
                    "source": describe(a, x),
                }),
                at: a.object(x).name.clone(),
                focus: None,
                primitive: Primitive::Choice,
                instructions: Some(Value::String(format!(
                    "Which object of {} corresponds to the source object: the same kind of case, the same step in handling it? Judge by meaning (descriptions, the options it offers, how it is reached), not by name. If none of them does, answer none of these.",
                    b.name()
                ))),
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
                let (answer, _) = judge.judge(req).await?;
                let Answer::Choice(d) = answer else {
                    return Err(ModelError::Decode("expected a choice".into()));
                };
                Ok::<_, ModelError>(Judged {
                    targets: targets.into_iter().zip(d.arrows).collect(),
                    none: d.none_of_these,
                    confidence: d.confidence,
                })
            })
        })
        .collect();
    let answers = crate::ensemble::join_all(futs).await;
    let mut out: Vec<Option<Judged>> = vec![None; admissible.len()];
    for (i, r) in asks.into_iter().zip(answers) {
        out[i] = Some(r?);
    }
    Ok(out)
}
