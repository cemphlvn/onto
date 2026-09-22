//! Live model clients: Jev (TypeSafe System One) as the chooser, any
//! OpenRouter chat model as the proposer.

use std::collections::BTreeMap;
use std::time::Duration;

use onto_core::category::NONE_OF_THESE;
use onto_core::walk::{Distribution, Proposal};
use serde_json::{Value, json};

use crate::model::{ChoiceRequest, Chooser, ModelError, ProposalRequest, Proposer, Usage};

const MAX_ATTEMPTS: u32 = 4;

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
                let v = resp.json::<Value>().await?;
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
            Err(e) => ModelError::Http(e),
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

impl Chooser for Jev {
    fn name(&self) -> String {
        format!("jev:{}", self.model)
    }

    async fn choose(&self, req: ChoiceRequest) -> Result<(Distribution, Usage), ModelError> {
        if req.frame.len() > 254 {
            return Err(ModelError::FrameTooWide(req.frame.len()));
        }
        let mut criteria: BTreeMap<&str, String> = req
            .frame
            .iter()
            .map(|c| {
                (
                    c.arrow.as_str(),
                    format!("follow `{}` to {}", c.arrow, c.to),
                )
            })
            .collect();
        criteria.insert(
            NONE_OF_THESE,
            "none of the listed arrows fits the goal from here".into(),
        );
        let body = json!({
            "model": self.model,
            "state": {
                "goal": req.goal,
                "at": req.at,
                "path_so_far": req.path_so_far,
            },
            "questions": {
                "next": {
                    "type": "choice",
                    "instructions": "The walk is at `at`, working toward `goal`. Which arrow should it follow next? Choose none_of_these if no listed arrow fits the goal.",
                    "criteria": criteria,
                }
            }
        });
        let (v, attempts) = post_json(&self.http, &self.url, &self.key, &body).await?;
        let answer = &v["answers"]["next"];
        let probs = answer["probabilities"]
            .as_object()
            .ok_or_else(|| ModelError::Decode(clip(&format!("no probabilities in {v}"))))?;
        let p = |k: &str| probs.get(k).and_then(Value::as_f64).unwrap_or(0.0) as f32;
        let d = Distribution {
            arrows: req.frame.iter().map(|c| p(&c.arrow)).collect(),
            none_of_these: p(NONE_OF_THESE),
            confidence: answer["confidence"].as_f64().map(|c| c as f32),
        };
        let usage = Usage {
            input_tokens: v["usage"]["input_tokens"].as_u64().unwrap_or(0),
            output_tokens: v["usage"]["output_tokens"].as_u64().unwrap_or(0),
            attempts,
        };
        Ok((d, usage))
    }
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
The walk is stuck at object `at`: its outgoing arrows (`frame`) do not cover the goal, for the stated `reason`. \
Propose 1 to 3 new arrows leaving `at` that would let the walk continue toward the goal. \
Each target is an existing object from `known_objects` when one fits, otherwise a new object name in PascalCase. \
Arrow names are short snake_case verbs. New arrows must not overlap each other or the existing frame \
(the frame should stay mutually exclusive). Reply with JSON only.";

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
                        "required": ["arrow", "target", "rationale"],
                        "properties": {
                            "arrow": {"type": "string"},
                            "target": {"type": "string"},
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
                rationale: p["rationale"].as_str().unwrap_or_default().to_owned(),
            })
            .collect();
        let usage = Usage {
            input_tokens: v["usage"]["prompt_tokens"].as_u64().unwrap_or(0),
            output_tokens: v["usage"]["completion_tokens"].as_u64().unwrap_or(0),
            attempts,
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
