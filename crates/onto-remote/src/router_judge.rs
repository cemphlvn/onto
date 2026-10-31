//! Any OpenRouter chat model as a System-1 judge, behind the same `Judge`
//! contract as Jev: for comparing judges on the same frames
//! (`--judge-model openrouter:<model>`). Choice frames only.
//!
//! The options are lettered and the model answers with one letter. When
//! the provider returns token log-probabilities, the distribution is read
//! from the letters' probabilities on the first answer token, the nearest
//! thing to Jev's calibrated read; otherwise the model is asked again to
//! state a probability per letter (a verbalized distribution, usually
//! less calibrated). `Distribution::confidence` is `None` either way.
//! `Usage::questions` counts the requests an answer took: 1 (letter
//! log-probabilities, or stated once the probe is known to fail) or 2
//! (probed, then stated). The first request turns
//! reasoning off (a reasoning model spends a short budget thinking and
//! returns no letter) and asks OpenRouter for a provider that honours
//! log-probabilities; when there is none it fails and the stated way is
//! taken. The judge remembers the first definite answer: once a provider
//! has refused log-probabilities (a 4xx other than 429) or answered
//! without them, later frames go straight to the stated way, one request
//! each.

use onto_core::Primitive;
use onto_core::walk::{Answer, Distribution};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicU8, Ordering};

use onto_models::{FrameRequest, Judge, ModelError, Usage};

use super::{clip, post_json};

const URL: &str = "https://openrouter.ai/api/v1/chat/completions";
const LETTERS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXY";
const NONE_LETTER: &str = "Z";

pub struct RouterJudge {
    http: reqwest::Client,
    key: String,
    model: String,
    /// Whether the model has letter log-probabilities: `UNKNOWN` until a
    /// first answer says, then `YES` or `NO`.
    logprobs: AtomicU8,
}

const UNKNOWN: u8 = 0;
const YES: u8 = 1;
const NO: u8 = 2;

impl RouterJudge {
    /// `spec` is `openrouter:<model>`; reads `OPENROUTER_API_KEY`.
    pub fn parse(http: reqwest::Client, spec: &str) -> Option<Self> {
        let model = spec.strip_prefix("openrouter:")?;
        Some(Self {
            http,
            key: std::env::var("OPENROUTER_API_KEY").ok()?,
            model: model.to_owned(),
            logprobs: AtomicU8::new(UNKNOWN),
        })
    }

    fn prompt(&self, req: &FrameRequest) -> String {
        let mut options = String::new();
        for (i, c) in req.candidates.iter().enumerate() {
            let text = match c.describe() {
                Value::String(s) => s,
                other => other.to_string(),
            };
            options.push_str(&format!("{}. {}: {}\n", LETTERS[i] as char, c.arrow, text));
        }
        options.push_str(&format!("{NONE_LETTER}. none of the options above fits\n"));
        format!(
            "Situation (JSON):\n{}\n\nQuestion: {}\n\nOptions:\n{options}",
            serde_json::to_string(&req.state).unwrap_or_default(),
            req.instructions
                .as_ref()
                .map_or("Which option fits?".to_owned(), |v| v
                    .as_str()
                    .map_or(v.to_string(), str::to_owned)),
        )
    }

    async fn call(&self, body: Value) -> Result<(Value, Usage), ModelError> {
        let (v, attempts) = post_json(&self.http, URL, &self.key, &body).await?;
        let usage = Usage {
            input_tokens: v["usage"]["prompt_tokens"].as_u64().unwrap_or(0),
            output_tokens: v["usage"]["completion_tokens"].as_u64().unwrap_or(0),
            attempts,
            questions: 1,
        };
        Ok((v, usage))
    }

    fn letters(&self, n: usize) -> Vec<String> {
        (0..n)
            .map(|i| (LETTERS[i] as char).to_string())
            .chain([NONE_LETTER.to_owned()])
            .collect()
    }
}

/// Normalizes non-negative weights over the options and "none"; `None`
/// when they carry no mass.
fn distribution(weights: &[f64]) -> Option<Distribution> {
    let total: f64 = weights.iter().sum();
    if total.is_nan() || total <= 0.0 {
        return None;
    }
    let (arrows, none) = weights.split_at(weights.len() - 1);
    Some(Distribution {
        arrows: arrows.iter().map(|w| (w / total) as f32).collect(),
        none_of_these: (none[0] / total) as f32,
        confidence: None,
    })
}

impl Judge for RouterJudge {
    fn name(&self) -> String {
        format!("openrouter:{}", self.model)
    }

    async fn judge(&self, req: FrameRequest) -> Result<(Answer, Usage), ModelError> {
        if req.primitive != Primitive::Choice {
            return Err(ModelError::Unsupported {
                model: self.name(),
                primitive: "noul or score",
            });
        }
        if req.candidates.len() > LETTERS.len() {
            return Err(ModelError::FrameTooWide(req.candidates.len()));
        }
        let letters = self.letters(req.candidates.len());
        let prompt = self.prompt(&req);

        // 1. One letter, with log-probabilities when the provider has them.
        let mut usage = Usage::default();
        let probed = self.logprobs.load(Ordering::Relaxed) != NO;
        if probed {
            let probe = self
            .call(json!({
                "model": self.model,
                "messages": [
                    {"role": "system", "content": "You judge which option fits a situation. Answer with the option's letter only, nothing else."},
                    {"role": "user", "content": prompt},
                ],
                "max_tokens": 4,
                "temperature": 0,
                "logprobs": true,
                "top_logprobs": 20,
                "reasoning": {"enabled": false},
                "provider": {"require_parameters": true},
            }))
            .await;
            let v = match probe {
                Ok((v, u)) => {
                    usage = u;
                    v
                }
                Err(ModelError::Status { status, .. })
                    if (400..500).contains(&status) && status != 429 =>
                {
                    self.logprobs.store(NO, Ordering::Relaxed);
                    Value::Null
                }
                Err(_) => Value::Null,
            };
            if let Some(top) = v["choices"][0]["logprobs"]["content"][0]["top_logprobs"].as_array()
            {
                let mut w = vec![0.0f64; letters.len()];
                for t in top {
                    let token = t["token"].as_str().unwrap_or("").trim().trim_matches('.');
                    if let (Some(k), Some(lp)) = (
                        letters.iter().position(|l| l == token),
                        t["logprob"].as_f64(),
                    ) {
                        w[k] += lp.exp();
                    }
                }
                if let Some(d) = distribution(&w) {
                    self.logprobs.store(YES, Ordering::Relaxed);
                    return Ok((Answer::Choice(d), usage));
                }
            } else if !v.is_null() {
                self.logprobs.store(NO, Ordering::Relaxed);
            }
        }

        // 2. No log-probabilities: ask for a stated distribution.
        let (v, more) = self
            .call(json!({
                "model": self.model,
                "messages": [
                    {"role": "system", "content": "You judge which option fits a situation. Reply with JSON only: an object mapping every option letter to your probability that it is the right one; the probabilities sum to 1."},
                    {"role": "user", "content": prompt},
                ],
                "temperature": 0,
                "response_format": {"type": "json_object"},
            }))
            .await?;
        usage.input_tokens += more.input_tokens;
        usage.output_tokens += more.output_tokens;
        usage.attempts += more.attempts;
        usage.questions = if probed { 2 } else { 1 };
        let text = v["choices"][0]["message"]["content"].as_str().unwrap_or("");
        let start = text.find('{').unwrap_or(0);
        let end = text.rfind('}').map_or(text.len(), |e| e + 1);
        let stated: Value = serde_json::from_str(&text[start..end])
            .map_err(|e| ModelError::Decode(clip(&format!("{e}: {text}"))))?;
        let w: Vec<f64> = letters
            .iter()
            .map(|l| stated[l].as_f64().unwrap_or(0.0).max(0.0))
            .collect();
        let d = distribution(&w)
            .ok_or_else(|| ModelError::Decode(clip(&format!("no probabilities: {text}"))))?;
        Ok((Answer::Choice(d), usage))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weights_become_a_distribution_with_none_last() {
        let d = distribution(&[3.0, 1.0, 0.0, 1.0]).unwrap();
        assert_eq!(d.arrows, vec![0.6, 0.2, 0.0]);
        assert!((d.none_of_these - 0.2).abs() < 1e-6);
        assert!(distribution(&[0.0, 0.0]).is_none());
    }
}
