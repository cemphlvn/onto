//! tkgd (fonto's local TKG inference server, `POST /tkg/puanla`) as a
//! System-1 judge, for benchmarking the TKG unit models on onto walks.
//!
//! TKG models are not instruction-tuned and have no option letters, so a
//! frame is read by likelihood: the case is the user's message, each
//! option's own words are scored as the assistant's reply, and the score is
//! taken relative to the same reply with no case (PMI, contextual
//! calibration, as `onto-local` does for wide frames). A softmax over the
//! options gives the distribution; `none_of_these` gets no mass, since no
//! option text stands for it.
//!
//! The models read Turkish only (onto's morphology tokenizer): the graph's
//! arrow instructions and the case's text must be Turkish.

use std::collections::HashMap;
use std::sync::Mutex;

use onto_core::Primitive;
use onto_core::walk::{Answer, Distribution};
use serde_json::{Value, json};

use onto_models::{Candidate, Critic, FrameRequest, Judge, ModelError, NoulQuestion, Usage};

use crate::{clip, post_json};

pub struct Tkgd {
    http: reqwest::Client,
    url: String,
    /// The tkgd model name; `None` is the server's default.
    model: Option<String>,
    calibrate: bool,
    /// Scores of each option set with no case, computed once per frame.
    priors: Mutex<HashMap<Vec<String>, Vec<f64>>>,
}

impl Tkgd {
    /// `tkgd:<model>` (calibrated) or `tkgd-raw:<model>` (plain
    /// likelihood); an empty model is the server's default. The server is
    /// at `TKGD_URL` (default `http://127.0.0.1:7470`).
    pub fn parse(http: reqwest::Client, spec: &str) -> Option<Self> {
        let (calibrate, model) = if let Some(m) = spec.strip_prefix("tkgd:") {
            (true, m)
        } else {
            (false, spec.strip_prefix("tkgd-raw:")?)
        };
        let base = std::env::var("TKGD_URL").unwrap_or_else(|_| "http://127.0.0.1:7470".into());
        Some(Self {
            http,
            url: format!("{}/tkg/puanla", base.trim_end_matches('/')),
            model: (!model.is_empty()).then(|| model.to_owned()),
            calibrate,
            priors: Mutex::new(HashMap::new()),
        })
    }

    /// log P(option | context) per option, and the units scored.
    async fn score(
        &self,
        context: &[Value],
        options: &[String],
    ) -> Result<(Vec<f64>, u64, u32), ModelError> {
        let mut body = json!({"baglam": context, "secenekler": options});
        if let Some(m) = &self.model {
            body["model"] = json!(m);
        }
        let (v, attempts) = post_json(&self.http, &self.url, "", &body).await?;
        let scores = v["puanlar"]
            .as_array()
            .filter(|p| p.len() == options.len())
            .ok_or_else(|| ModelError::Decode(clip(&format!("unexpected tkgd reply: {v}"))))?;
        let logp = scores
            .iter()
            .map(|p| p["logp"].as_f64().unwrap_or(f64::NEG_INFINITY))
            .collect();
        let units = scores
            .iter()
            .map(|p| p["birim"].as_u64().unwrap_or(0))
            .sum();
        Ok((logp, units, attempts))
    }

    async fn prior(&self, options: &[String]) -> Result<(Vec<f64>, u64), ModelError> {
        if let Some(p) = self.priors.lock().unwrap().get(options) {
            return Ok((p.clone(), 0));
        }
        let (p, units, _) = self.score(&[], options).await?;
        self.priors
            .lock()
            .unwrap()
            .insert(options.to_vec(), p.clone());
        Ok((p, units))
    }
}

/// The case as the user's message: its `goal` when it is text, else every
/// text value of the state, in order.
fn case_text(state: &Value) -> String {
    if let Some(g) = state.get("goal").and_then(Value::as_str) {
        return g.to_owned();
    }
    fn texts(v: &Value, out: &mut Vec<String>) {
        match v {
            Value::String(s) => out.push(s.clone()),
            Value::Array(a) => a.iter().for_each(|x| texts(x, out)),
            Value::Object(o) => o.values().for_each(|x| texts(x, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    texts(state, &mut out);
    out.join(". ")
}

/// An option as a reply: its text instruction, else the arrow's name.
fn option_text(c: &Candidate) -> String {
    match &c.instructions {
        Some(Value::String(t)) => t.clone(),
        _ => c.arrow.replace('_', " "),
    }
}

fn softmax(x: &[f64]) -> Vec<f32> {
    let m = x.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let e: Vec<f64> = x.iter().map(|v| (v - m).exp()).collect();
    let z: f64 = e.iter().sum();
    e.iter().map(|v| (v / z) as f32).collect()
}

impl Judge for Tkgd {
    fn name(&self) -> String {
        let kind = if self.calibrate { "tkgd" } else { "tkgd-raw" };
        format!("{kind}:{}", self.model.as_deref().unwrap_or("default"))
    }

    async fn judge(&self, req: FrameRequest) -> Result<(Answer, Usage), ModelError> {
        let unsupported = |p| ModelError::Unsupported {
            model: Judge::name(self),
            primitive: p,
        };
        match req.primitive {
            Primitive::Choice | Primitive::Score => {}
            Primitive::Noul => return Err(unsupported("noul")),
            Primitive::Split => return Err(unsupported("split")),
        }
        let options: Vec<String> = req.candidates.iter().map(option_text).collect();
        let context = [json!({"rol": "user", "metin": case_text(&req.state)})];
        let (mut s, mut units, attempts) = self.score(&context, &options).await?;
        if self.calibrate {
            let (prior, u) = self.prior(&options).await?;
            s.iter_mut().zip(&prior).for_each(|(x, p)| *x -= p);
            units += u;
        }
        let probs = softmax(&s);
        let answer = match req.primitive {
            Primitive::Score => Answer::Score {
                levels: probs,
                confidence: None,
            },
            _ => Answer::Choice(Distribution {
                arrows: probs,
                none_of_these: 0.0,
                confidence: None,
            }),
        };
        let usage = Usage {
            input_tokens: units,
            output_tokens: 0,
            attempts,
            questions: 1,
        };
        Ok((answer, usage))
    }
}

/// Yes/no questions have no reply text to score; supervision needs another
/// critic.
impl Critic for Tkgd {
    fn name(&self) -> String {
        Judge::name(self)
    }

    async fn nouls(
        &self,
        _state: Value,
        _questions: Vec<NoulQuestion>,
    ) -> Result<(Vec<f32>, Usage), ModelError> {
        Err(ModelError::Unsupported {
            model: Judge::name(self),
            primitive: "noul",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_case_is_its_goal_or_its_texts() {
        assert_eq!(
            case_text(&json!({"goal": "iki kez ücret alındı", "id": "L-1"})),
            "iki kez ücret alındı"
        );
        assert_eq!(case_text(&json!({"a": "bir", "b": ["iki", 3]})), "bir. iki");
    }

    #[test]
    fn specs_name_the_readout() {
        let http = reqwest::Client::new();
        assert!(
            Tkgd::parse(http.clone(), "tkgd:tkg-suyu-d128")
                .unwrap()
                .calibrate
        );
        assert!(!Tkgd::parse(http.clone(), "tkgd-raw:").unwrap().calibrate);
        assert!(Tkgd::parse(http, "local:x.gguf").is_none());
    }
}
