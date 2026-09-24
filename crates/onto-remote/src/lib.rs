//! Remote model clients: Jev (TypeSafe System One) as the judge and
//! critic, any OpenRouter chat model as the proposer. The engine calls
//! them concurrently, up to `judge_concurrency` / `proposer_concurrency`
//! requests in flight.

use std::time::Duration;

use onto_core::Primitive;
use onto_core::category::NONE_OF_THESE;
use onto_core::walk::{Answer, Distribution, Proposal};
use serde_json::{Value, json};

use onto_models::{
    Candidate, Critic, FrameRequest, Judge, ModelError, NoulQuestion, ProposalRequest, Proposer,
    Usage,
};

const MAX_ATTEMPTS: u32 = 4;

fn transport(e: reqwest::Error) -> ModelError {
    ModelError::Http(e.to_string())
}

/// POSTs JSON, retrying 429/529/5xx and transport errors with exponential
/// backoff (250ms, 500ms, 1s).
async fn post_json(
    http: &reqwest::Client,
    url: &str,
    key: &str,
    body: &Value,
) -> Result<(Value, u32), ModelError> {
    let mut attempt = 0;
    loop {
        attempt += 1;
        let result = http.post(url).bearer_auth(key).json(body).send().await;
        let retry = match result {
            Ok(resp) if resp.status().is_success() => {
                let v = resp.json::<Value>().await.map_err(transport)?;
                return Ok((v, attempt));
            }
            Ok(resp) => {
                let status = resp.status().as_u16();
                let body = clip(&resp.text().await.unwrap_or_default());
                let err = ModelError::Status { status, body };
                if !(status == 429 || status >= 500) {
                    return Err(err);
                }
                err
            }
            Err(e) => transport(e),
        };
        if attempt >= MAX_ATTEMPTS {
            return Err(retry);
        }
        tokio::time::sleep(Duration::from_millis(250 << (attempt - 1))).await;
    }
}

pub struct Jev {
    http: reqwest::Client,
    key: String,
    model: String,
    url: String,
}

impl Jev {
    /// Reads `TYPESAFE_API_KEY`; `model` defaults to `jev-latest`.
    pub fn from_env(http: reqwest::Client, model: Option<String>) -> Option<Self> {
        Some(Self {
            http,
            key: std::env::var("TYPESAFE_API_KEY").ok()?,
            model: model.unwrap_or_else(|| "jev-latest".into()),
            url: "https://api.typesafe.ai/v1/systemone".into(),
        })
    }
}

impl Judge for Jev {
    fn name(&self) -> String {
        format!("jev:{}", self.model)
    }

    async fn judge(&self, req: FrameRequest) -> Result<(Answer, Usage), ModelError> {
        let questions = render(&req)?;
        let n_questions = questions.as_object().map_or(0, |q| q.len() as u32);
        let body = json!({
            "model": self.model,
            "state": req.state,
            "questions": questions,
        });
        let (v, attempts) = post_json(&self.http, &self.url, &self.key, &body).await?;
        let answers = &v["answers"];
        let answer = read_answer(&req, answers)
            .ok_or_else(|| ModelError::Decode(clip(&format!("unexpected answers: {answers}"))))?;
        let usage = Usage {
            input_tokens: v["usage"]["input_tokens"].as_u64().unwrap_or(0),
            output_tokens: v["usage"]["output_tokens"].as_u64().unwrap_or(0),
            attempts,
            questions: n_questions,
        };
        Ok((answer, usage))
    }
}

impl Critic for Jev {
    fn name(&self) -> String {
        format!("jev:{}", self.model)
    }

    async fn nouls(
        &self,
        state: Value,
        questions: Vec<NoulQuestion>,
    ) -> Result<(Vec<f32>, Usage), ModelError> {
        let n = questions.len();
        let qs: serde_json::Map<String, Value> = questions
            .into_iter()
            .enumerate()
            .map(|(i, q)| {
                let mut v = json!({"type": "noul", "instructions": q.instructions});
                if let Some(c) = q.criteria {
                    v["criteria"] = c;
                }
                (format!("q{i}"), v)
            })
            .collect();
        let body = json!({"model": self.model, "state": state, "questions": qs});
        let (v, attempts) = post_json(&self.http, &self.url, &self.key, &body).await?;
        let ps = (0..n)
            .map(|i| {
                v["answers"][format!("q{i}")]["noul"]
                    .as_f64()
                    .map(|x| x as f32)
            })
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| {
                ModelError::Decode(clip(&format!("unexpected answers: {}", v["answers"])))
            })?;
        let usage = Usage {
            input_tokens: v["usage"]["input_tokens"].as_u64().unwrap_or(0),
            output_tokens: v["usage"]["output_tokens"].as_u64().unwrap_or(0),
            attempts,
            questions: n as u32,
        };
        Ok((ps, usage))
    }
}

/// How an arrow reads as an option, condition or level: its instructions
/// when declared (text gets the target appended; JSON is wrapped with it),
/// else a sentence built from its name.
fn describe(c: &Candidate) -> Value {
    match &c.instructions {
        Some(Value::String(t)) => json!(format!("{t} (leads to {})", c.to)),
        Some(structured) => json!({"leads_to": c.to, "description": structured}),
        None => json!(format!("follow `{}` to {}", c.arrow, c.to)),
    }
}

/// The frame as TypeSafe questions, one request per frame.
fn render(req: &FrameRequest) -> Result<Value, ModelError> {
    // After a fork, each branch handles one aspect of the case.
    let scope = if req.focus.is_some() {
        " This walk handles only the aspect in `focus`; judge for that aspect and ignore the case's other aspects."
    } else {
        ""
    };
    let question = |default: &str| match &req.instructions {
        Some(i) => json!({"question": i, "context": format!("{default}{scope}")}),
        None => json!(format!("{default}{scope}")),
    };
    Ok(match req.primitive {
        Primitive::Split => {
            // Split frames are never judged; the engine does not send them.
            return Err(ModelError::Unsupported {
                model: "jev".into(),
                primitive: "split",
            });
        }
        Primitive::Choice => {
            if req.candidates.len() > 254 {
                return Err(ModelError::FrameTooWide(req.candidates.len()));
            }
            let mut criteria: serde_json::Map<String, Value> = req
                .candidates
                .iter()
                .map(|c| (c.arrow.clone(), describe(c)))
                .collect();
            criteria.insert(
                NONE_OF_THESE.into(),
                json!("none of the listed options fits the goal from here"),
            );
            json!({"next": {
                "type": "choice",
                "instructions": question("The walk is at `at`, working toward `goal`. Which option should it follow next? Choose none_of_these if no listed option fits."),
                "criteria": criteria,
            }})
        }
        Primitive::Noul => {
            let mut qs = serde_json::Map::new();
            for (i, c) in req.candidates.iter().enumerate() {
                let mut instructions = json!({
                    "condition": describe(c),
                    "question": format!("Given `goal`, the case, and the walk so far (`hops`), does `condition` hold for this case?{scope}"),
                });
                if let Some(frame_question) = &req.instructions {
                    instructions["frame_question"] = frame_question.clone();
                }
                qs.insert(
                    format!("holds_{i}"),
                    json!({"type": "noul", "instructions": instructions}),
                );
            }
            if req.can_fork && !req.parallel && req.candidates.len() > 1 {
                let options: Vec<Value> = req.candidates.iter().map(describe).collect();
                qs.insert("fork".into(), json!({
                    "type": "noul",
                    "instructions": {
                        "options": options,
                        "question": "Suppose more than one of `options` holds for this case. Given `goal` and the walk so far (`hops`), should each holding option be pursued as its own parallel line of work?",
                    },
                    "criteria": {
                        "true": "They are independent aspects of the case; each needs its own handling.",
                        "false": "They are competing readings of the same thing; only the most likely should be followed.",
                    },
                }));
            }
            Value::Object(qs)
        }
        Primitive::Score => {
            let levels: Vec<Value> = req.candidates.iter().map(describe).collect();
            json!({"level": {
                "type": "score",
                "instructions": question("Where does this case fall on the scale, given `goal`?"),
                "criteria": levels,
            }})
        }
    })
}

fn read_answer(req: &FrameRequest, answers: &Value) -> Option<Answer> {
    let f = |v: &Value| v.as_f64().map(|x| x as f32);
    Some(match req.primitive {
        Primitive::Split => return None,
        Primitive::Choice => {
            let a = &answers["next"];
            let probs = a["probabilities"].as_object()?;
            let p = |k: &str| probs.get(k).and_then(f).unwrap_or(0.0);
            Answer::Choice(Distribution {
                arrows: req.candidates.iter().map(|c| p(&c.arrow)).collect(),
                none_of_these: p(NONE_OF_THESE),
                confidence: f(&a["confidence"]),
            })
        }
        Primitive::Noul => Answer::Noul {
            holds: (0..req.candidates.len())
                .map(|i| f(&answers[format!("holds_{i}")]["noul"]))
                .collect::<Option<Vec<_>>>()?,
            fork: f(&answers["fork"]["noul"]),
        },
        Primitive::Score => {
            let a = &answers["level"];
            let probs = a["probabilities"].as_object()?;
            Answer::Score {
                levels: (0..req.candidates.len())
                    .map(|i| probs.get(&i.to_string()).and_then(f).unwrap_or(0.0))
                    .collect(),
                confidence: f(&a["confidence"]),
            }
        }
    })
}

pub struct OpenRouter {
    http: reqwest::Client,
    key: String,
    model: String,
}

/// Default System-2 model; override with `--proposer-model` or
/// `ONTO_PROPOSER_MODEL`.
pub const DEFAULT_PROPOSER_MODEL: &str = "~openai/gpt-luna-latest";

impl OpenRouter {
    /// Reads `OPENROUTER_API_KEY`.
    pub fn from_env(http: reqwest::Client, model: Option<String>) -> Option<Self> {
        Some(Self {
            http,
            key: std::env::var("OPENROUTER_API_KEY").ok()?,
            model: model
                .or_else(|| std::env::var("ONTO_PROPOSER_MODEL").ok())
                .unwrap_or_else(|| DEFAULT_PROPOSER_MODEL.into()),
        })
    }
}

const PROPOSER_SYSTEM: &str = "You extend a category (objects and arrows) that a fast decision model walks. \
`state` is everything you may see of the case (declared by policy; fields not in it are withheld on purpose, do not ask for them). \
The walk is stuck at object `at`: its outgoing arrows (`frame`) do not cover the goal, for the stated `reason`. \
Propose 1 to 3 new arrows leaving `at` that would let the walk continue toward the goal. \
Each target is an existing object from `known_objects` when one fits, otherwise a new object name in PascalCase. \
A walk succeeds only when it reaches one of the `outcomes`; `reached_by` shows how this graph finishes a case \
(the handling action is an arrow INTO the outcome, whether or not the customer's problem is already solved). \
Mirror that shape: when handling the case is one action from `at`, target the outcome directly with an arrow named for the action; \
introduce a new object only when a genuinely different decision remains before the action. \
Never target an object on `path_so_far`: a learned arrow may not close a cycle and is refused. \
Never target an object in `sealed`: sealed objects are reached only by arrows people declared, so such a proposal is refused; \
name a new object instead (the case then needs a person, which is the right outcome for what policy does not cover). \
Arrow names are short snake_case verbs. Give each arrow an `about`: one sentence saying which cases it is for. \
New arrows must not overlap each other or the existing frame (read each existing arrow's instructions), \
and must not re-propose an existing arrow. When `focus` is set, this walk is one branch of a case with several \
independent aspects: propose arrows for the aspect in `focus` only. `pending_here` lists provisional arrows other cases already \
proposed at this object: when one of them fits this case, return it unchanged (same arrow name, target and about) \
instead of inventing a new name; propose new arrows only for what pending ones do not cover. Reply with JSON only.";

impl Proposer for OpenRouter {
    fn name(&self) -> String {
        format!("openrouter:{}", self.model)
    }

    async fn propose(&self, req: ProposalRequest) -> Result<(Vec<Proposal>, Usage), ModelError> {
        let schema = json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["proposals"],
            "properties": {
                "proposals": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["arrow", "target", "about", "rationale"],
                        "properties": {
                            "arrow": {"type": "string"},
                            "target": {"type": "string"},
                            "about": {"type": "string"},
                            "rationale": {"type": "string"}
                        }
                    }
                }
            }
        });
        let body = json!({
            "model": self.model,
            "temperature": 0.2,
            "max_tokens": 600,
            // Reasoning models otherwise spend the budget before answering.
            "reasoning": {"effort": "low"},
            "messages": [
                {"role": "system", "content": PROPOSER_SYSTEM},
                {"role": "user", "content": serde_json::to_string(&req).expect("serializable")},
            ],
            "response_format": {
                "type": "json_schema",
                "json_schema": {"name": "proposals", "strict": true, "schema": schema}
            }
        });
        let url = "https://openrouter.ai/api/v1/chat/completions";
        let (v, attempts) = post_json(&self.http, url, &self.key, &body).await?;
        let content = v["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| {
                let finish = v["choices"][0]["finish_reason"].as_str().unwrap_or("?");
                ModelError::Decode(format!("no message content (finish_reason: {finish})"))
            })?;
        let parsed: Value = serde_json::from_str(strip_fences(content))
            .map_err(|e| ModelError::Decode(clip(&format!("{e}: {content}"))))?;
        let proposals = parsed["proposals"]
            .as_array()
            .ok_or_else(|| ModelError::Decode(clip(&format!("no proposals in {content}"))))?
            .iter()
            .map(|p| Proposal {
                arrow: p["arrow"].as_str().unwrap_or_default().to_owned(),
                src: req.at.clone(),
                dst: p["target"].as_str().unwrap_or_default().to_owned(),
                about: p["about"].as_str().unwrap_or_default().to_owned(),
                rationale: p["rationale"].as_str().unwrap_or_default().to_owned(),
                ..Default::default()
            })
            .collect();
        let usage = Usage {
            input_tokens: v["usage"]["prompt_tokens"].as_u64().unwrap_or(0),
            output_tokens: v["usage"]["completion_tokens"].as_u64().unwrap_or(0),
            attempts,
            questions: 1,
        };
        Ok((proposals, usage))
    }
}

/// Keeps error messages log-sized.
fn clip(s: &str) -> String {
    const MAX: usize = 300;
    match s.char_indices().nth(MAX) {
        Some((i, _)) => format!("{}… ({} bytes)", &s[..i], s.len()),
        None => s.to_owned(),
    }
}

/// Some models wrap JSON in a Markdown fence despite the schema.
fn strip_fences(s: &str) -> &str {
    let s = s.trim();
    s.strip_prefix("```json")
        .or_else(|| s.strip_prefix("```"))
        .and_then(|r| r.strip_suffix("```"))
        .map_or(s, str::trim)
}
