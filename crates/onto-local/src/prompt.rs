//! Frames as lettered prompts, and letter probabilities back as answers.
//!
//! The questions are the ones the Jev adapter asks (`onto-remote`), in the
//! same words, so a local model and Jev judge the same thing; only the
//! readout differs (option-letter logits instead of a hosted model's
//! probabilities).

use onto_core::Primitive;
use onto_core::walk::{Answer, Distribution};
use onto_models::{Candidate, FrameRequest, ModelError, NoulQuestion};
use serde_json::Value;

/// Option labels: single letters, each one token in common vocabularies.
/// Frames wider than this are read by likelihood: each option's own text
/// is scored as the answer (`llama.rs`).
pub const LABELS: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// The widest frame a prompt can hold (Jev's Choice limit, 254 + none).
pub const MAX_OPTIONS: usize = 255;

pub const SYSTEM: &str = "You are a careful decision function. Read the options, the question and the case state (JSON), then answer with exactly one option. Judge only from the state; do not invent facts.";

/// One question to score: chat messages ending where the answer goes.
/// `labels` are read as single tokens (letters); `answers`, when set,
/// are the options' own texts, each scored as the whole answer.
#[derive(Clone, Debug)]
pub struct Prompt {
    pub system: String,
    pub user: String,
    pub labels: Vec<String>,
    pub answers: Option<Vec<String>>,
    /// For answers: the same prompt with a content-free case (`goal` =
    /// "N/A"); scores are taken relative to it (contextual calibration,
    /// Zhao et al. 2021), removing each option's prior pull.
    pub baseline: Option<String>,
}

/// What stays the same across cases comes first (the options, the
/// question), the case last: consecutive cases at one frame share every
/// token up to their state, which the scorer keeps cached.
fn lettered(state: &Value, question: &Value, options: &[Value]) -> Result<Prompt, ModelError> {
    let shorts: Vec<String> = options.iter().map(short).collect();
    lettered_with(state, question, options, &shorts)
}

/// An option's plain words for likelihood answers: its text instruction
/// without the `(leads to …)` suffix `describe` adds, or the arrow's words.
fn short(v: &Value) -> String {
    match v {
        Value::String(s) => s
            .rsplit_once(" (leads to ")
            .map_or(s.as_str(), |(t, _)| t)
            .to_owned(),
        other => other.to_string(),
    }
}

fn content_free(state: &Value) -> Value {
    let mut s = state.clone();
    if let Some(o) = s.as_object_mut() {
        o.insert("goal".into(), Value::String("N/A".into()));
        o.remove("case");
    }
    s
}

fn lettered_with(
    state: &Value,
    question: &Value,
    options: &[Value],
    shorts: &[String],
) -> Result<Prompt, ModelError> {
    if options.len() > MAX_OPTIONS {
        return Err(ModelError::FrameTooWide(options.len()));
    }
    let text = |v: &Value| match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    let wide = options.len() > LABELS.len();
    let texts: Vec<String> = if wide {
        shorts.to_vec()
    } else {
        options.iter().map(text).collect()
    };
    let labels: Vec<String> = if wide {
        (1..=options.len()).map(|i| i.to_string()).collect()
    } else {
        LABELS
            .chars()
            .take(options.len())
            .map(String::from)
            .collect()
    };
    let mut head = String::from("Options:\n");
    for (l, t) in labels.iter().zip(&texts) {
        if wide {
            head.push_str(&format!("- {t}\n"));
        } else {
            head.push_str(&format!("{l}) {t}\n"));
        }
    }
    let how = if wide {
        "Answer with the exact text of one option."
    } else {
        "Answer with the option's letter."
    };
    let user = |state: &Value| {
        format!(
            "{head}\nQuestion: {}\n\nState:\n{}\n\n{how}",
            text(question),
            serde_json::to_string_pretty(state).unwrap_or_default(),
        )
    };
    Ok(Prompt {
        system: SYSTEM.into(),
        user: user(state),
        labels,
        baseline: wide.then(|| user(&content_free(state))),
        answers: wide.then_some(texts),
    })
}

fn yes_no(state: &Value, question: Value, yes: Option<&Value>, no: Option<&Value>) -> Prompt {
    let yes = yes.map_or("yes".into(), |v| {
        format!("yes: {}", v.as_str().unwrap_or(&v.to_string()))
    });
    let no = no.map_or("no".into(), |v| {
        format!("no: {}", v.as_str().unwrap_or(&v.to_string()))
    });
    lettered(state, &question, &[Value::String(yes), Value::String(no)])
        .expect("two options always fit")
}

/// The prompts for one frame: one for a choice or score frame; one per
/// condition (plus the fork question) for a noul frame.
pub fn frame(req: &FrameRequest) -> Result<Vec<Prompt>, ModelError> {
    let scope = if req.focus.is_some() {
        " This walk handles only the aspect in `focus`; judge for that aspect and ignore the case's other aspects."
    } else {
        ""
    };
    let question = |default: &str| match &req.instructions {
        Some(i) => serde_json::json!({"question": i, "context": format!("{default}{scope}")}),
        None => Value::String(format!("{default}{scope}")),
    };
    Ok(match req.primitive {
        Primitive::Split => {
            return Err(ModelError::Unsupported {
                model: "local".into(),
                primitive: "split",
            });
        }
        Primitive::Choice => {
            let mut options: Vec<Value> = req.candidates.iter().map(Candidate::describe).collect();
            options.push(Value::String(
                "none of the listed options fits the goal from here".into(),
            ));
            vec![lettered(
                &req.state,
                &question(
                    "The walk is at `at`, working toward `goal`. Which option should it follow next? Choose the last option if no listed option fits.",
                ),
                &options,
            )?]
        }
        Primitive::Noul => {
            let mut prompts: Vec<Prompt> = req
                .candidates
                .iter()
                .map(|c| {
                    let mut q = serde_json::json!({
                        "condition": c.describe(),
                        "question": format!("Given `goal`, the case, and the walk so far (`hops`), does `condition` hold for this case?{scope}"),
                    });
                    if let Some(f) = &req.instructions {
                        q["frame_question"] = f.clone();
                    }
                    yes_no(&req.state, q, None, None)
                })
                .collect();
            if req.can_fork && !req.parallel && req.candidates.len() > 1 {
                let options: Vec<Value> = req.candidates.iter().map(Candidate::describe).collect();
                prompts.push(yes_no(
                    &req.state,
                    serde_json::json!({
                        "options": options,
                        "question": "Suppose more than one of `options` holds for this case. Given `goal` and the walk so far (`hops`), should each holding option be pursued as its own parallel line of work?",
                    }),
                    Some(&Value::String("they are independent aspects of the case; each needs its own handling".into())),
                    Some(&Value::String("they are competing readings of the same thing; only the most likely should be followed".into())),
                ));
            }
            prompts
        }
        Primitive::Score => {
            let levels: Vec<Value> = req.candidates.iter().map(Candidate::describe).collect();
            vec![lettered(
                &req.state,
                &question("Where does this case fall on the scale, given `goal`?"),
                &levels,
            )?]
        }
    })
}

/// A supervisor's yes/no question.
pub fn critic(state: &Value, q: &NoulQuestion) -> Prompt {
    let (yes, no) = match &q.criteria {
        Some(c) => (c.get("true"), c.get("false")),
        None => (None, None),
    };
    yes_no(state, q.instructions.clone(), yes, no)
}

/// Letter probabilities (one vector per prompt of [`frame`]) as the answer
/// the engine expects from any judge.
pub fn read(req: &FrameRequest, probs: &[&[f32]]) -> Option<Answer> {
    Some(match req.primitive {
        Primitive::Split => return None,
        Primitive::Choice => {
            let p = probs.first()?;
            let n = req.candidates.len();
            Answer::Choice(Distribution {
                arrows: p.get(..n)?.to_vec(),
                none_of_these: *p.get(n)?,
                confidence: None,
            })
        }
        Primitive::Noul => {
            let n = req.candidates.len();
            let holds = probs.get(..n)?.iter().map(|p| p[0]).collect();
            Answer::Noul {
                holds,
                fork: probs.get(n).map(|p| p[0]),
            }
        }
        Primitive::Score => Answer::Score {
            levels: probs.first()?.to_vec(),
            confidence: None,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use onto_models::Candidate;
    use serde_json::json;

    fn req(primitive: Primitive, n: usize) -> FrameRequest {
        FrameRequest {
            state: json!({"goal": "my parcel never arrived"}),
            at: "Ticket".into(),
            focus: None,
            primitive,
            instructions: None,
            candidates: (0..n)
                .map(|i| Candidate {
                    arrow: format!("a{i}"),
                    to: format!("T{i}"),
                    instructions: Some(json!(format!("option {i}"))),
                    level: None,
                })
                .collect(),
            can_fork: true,
            parallel: false,
        }
    }

    #[test]
    fn a_choice_is_one_prompt_with_none_of_these_last() {
        let ps = frame(&req(Primitive::Choice, 3)).unwrap();
        assert_eq!(ps.len(), 1);
        assert_eq!(ps[0].labels, ["A", "B", "C", "D"]);
        assert!(ps[0].user.contains("D) none of the listed options"));
        let a = read(&req(Primitive::Choice, 3), &[&[0.1, 0.7, 0.1, 0.1]]).unwrap();
        let Answer::Choice(d) = a else { panic!() };
        assert_eq!(d.arrows, [0.1, 0.7, 0.1]);
        assert_eq!(d.none_of_these, 0.1);
    }

    #[test]
    fn a_noul_asks_each_condition_and_the_fork() {
        let r = req(Primitive::Noul, 2);
        let ps = frame(&r).unwrap();
        assert_eq!(ps.len(), 3, "two conditions and the fork question");
        let a = read(&r, &[&[0.9, 0.1], &[0.2, 0.8], &[0.6, 0.4]]).unwrap();
        let Answer::Noul { holds, fork } = a else {
            panic!()
        };
        assert_eq!(holds, [0.9, 0.2]);
        assert_eq!(fork, Some(0.6));
    }

    #[test]
    fn wide_frames_are_read_by_likelihood() {
        let ps = frame(&req(Primitive::Choice, 150)).unwrap();
        let answers = ps[0].answers.as_ref().unwrap();
        assert_eq!(answers.len(), 151);
        assert_eq!(answers[0], "option 0");
        assert!(ps[0].baseline.as_ref().unwrap().contains("\"N/A\""));
        assert!(ps[0].user.contains("- none of the listed options"));
        assert!(
            frame(&req(Primitive::Choice, 3)).unwrap()[0]
                .answers
                .is_none()
        );
        let too_wide = req(Primitive::Choice, MAX_OPTIONS);
        assert!(matches!(frame(&too_wide), Err(ModelError::FrameTooWide(_))));
    }

    #[test]
    fn options_come_before_the_state() {
        let ps = frame(&req(Primitive::Choice, 3)).unwrap();
        let (o, s) = (
            ps[0].user.find("Options:").unwrap(),
            ps[0].user.find("State:").unwrap(),
        );
        assert!(o < s, "the static part is the cached prefix");
    }
}
