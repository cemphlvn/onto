//! Offline models for tests and demos, over the contract in [`onto_models`]
//! (re-exported here, so `onto_runtime::model::*` names both).

use std::time::Duration;

use onto_core::Primitive;
use onto_core::walk::{Answer, Distribution, Proposal};
use serde_json::Value;

pub use onto_models::*;

/// Offline critic, deterministic, reading the structured instructions the
/// supervisor sends: duplicate when the targets or arrow names match
/// (case- and separator-insensitive); overlap never; a rule is violated
/// when the rule and the proposal share a word longer than five letters.
pub struct MockCritic;

impl Critic for MockCritic {
    fn name(&self) -> String {
        "mock-critic".into()
    }

    async fn nouls(
        &self,
        _: Value,
        questions: Vec<NoulQuestion>,
    ) -> Result<(Vec<f32>, Usage), ModelError> {
        let key = |v: &Value| -> String {
            v.as_str()
                .unwrap_or_default()
                .chars()
                .filter(|c| c.is_alphanumeric())
                .flat_map(char::to_lowercase)
                .collect()
        };
        let words = |v: &Value| -> Vec<String> {
            v.to_string()
                .split(|c: char| !c.is_alphanumeric())
                .filter(|w| w.len() > 5)
                .map(str::to_lowercase)
                .collect()
        };
        let ps = questions
            .iter()
            .map(|q| {
                let i = &q.instructions;
                let (p, e) = (&i["proposal"], &i["existing"]);
                match i["check"].as_str() {
                    Some("duplicate") => {
                        let same =
                            key(&p["to"]) == key(&e["to"]) || key(&p["arrow"]) == key(&e["arrow"]);
                        if same { 0.9 } else { 0.1 }
                    }
                    Some("rule") => {
                        let rule = words(&i["rule"]);
                        if words(p).iter().any(|w| rule.contains(w)) {
                            0.9
                        } else {
                            0.1
                        }
                    }
                    _ => 0.1,
                }
            })
            .collect();
        Ok((
            ps,
            Usage {
                attempts: 1,
                questions: questions.len() as u32,
                ..Usage::default()
            },
        ))
    }
}

/// Offline judge: an arrow applies when its name, target or instruction
/// text appears in the goal. Several arrows applying and the goal saying
/// "and" means independent aspects (fork). After a fixed latency.
pub struct MockJudge {
    pub latency: Duration,
}

impl MockJudge {
    fn hit(goal: &str, c: &Candidate) -> bool {
        let words = [c.arrow.as_str(), c.to.as_str()];
        words.iter().any(|w| goal.contains(&w.to_lowercase()))
            || c.instructions
                .as_ref()
                .and_then(Value::as_str)
                .is_some_and(|t| {
                    t.split(|ch: char| !ch.is_alphanumeric())
                        .any(|w| w.len() > 3 && goal.contains(&w.to_lowercase()))
                })
    }
}

/// The mock judge critiques like [`MockCritic`] (open world reviews with
/// the judge).
impl Critic for MockJudge {
    fn name(&self) -> String {
        MockCritic.name()
    }

    async fn nouls(
        &self,
        state: Value,
        questions: Vec<NoulQuestion>,
    ) -> Result<(Vec<f32>, Usage), ModelError> {
        MockCritic.nouls(state, questions).await
    }
}

impl Judge for MockJudge {
    fn name(&self) -> String {
        format!("mock({}ms)", self.latency.as_millis())
    }

    async fn judge(&self, req: FrameRequest) -> Result<(Answer, Usage), ModelError> {
        tokio::time::sleep(self.latency).await;
        let goal = req.state["goal"]
            .as_str()
            .unwrap_or_default()
            .to_lowercase();
        let hits: Vec<bool> = req.candidates.iter().map(|c| Self::hit(&goal, c)).collect();
        let first = hits.iter().position(|h| *h);
        let answer = match req.primitive {
            Primitive::Choice => {
                let mut arrows = vec![0.0; hits.len()];
                if let Some(i) = first {
                    arrows[i] = 0.95;
                }
                Answer::Choice(Distribution {
                    arrows,
                    none_of_these: if first.is_some() { 0.05 } else { 1.0 },
                    confidence: Some(0.9),
                })
            }
            Primitive::Noul => Answer::Noul {
                holds: hits.iter().map(|h| if *h { 0.95 } else { 0.05 }).collect(),
                fork: req
                    .can_fork
                    .then(|| if goal.contains(" and ") { 0.9 } else { 0.1 }),
            },
            Primitive::Split => Answer::Noul {
                holds: vec![1.0; hits.len()],
                fork: None,
            },
            Primitive::Score => {
                let mut levels = vec![0.0; hits.len()];
                levels[first.unwrap_or(0)] = 1.0;
                Answer::Score {
                    levels,
                    confidence: Some(if first.is_some() { 0.9 } else { 0.2 }),
                }
            }
        };
        let questions = match req.primitive {
            Primitive::Noul => hits.len() as u32 + u32::from(req.can_fork),
            _ => 1,
        };
        Ok((
            answer,
            Usage {
                attempts: 1,
                questions,
                ..Usage::default()
            },
        ))
    }
}

/// Offline proposer: reuses a pending proposal whose target the goal
/// mentions; otherwise suggests one arrow to a new object named after the
/// goal's last word. After a fixed latency.
pub struct MockProposer {
    pub latency: Duration,
}

impl Proposer for MockProposer {
    fn name(&self) -> String {
        format!("mock({}ms)", self.latency.as_millis())
    }

    async fn propose(&self, req: ProposalRequest) -> Result<(Vec<Proposal>, Usage), ModelError> {
        tokio::time::sleep(self.latency).await;
        let goal = req.state["goal"]
            .as_str()
            .unwrap_or_default()
            .to_lowercase();
        if let Some(p) = req
            .pending_here
            .iter()
            .find(|p| goal.contains(&p.target.to_lowercase()))
        {
            let reused = Proposal {
                arrow: p.arrow.clone(),
                src: req.at.clone(),
                dst: p.target.clone(),
                about: p.about.clone(),
                rationale: format!("same kind of case as walk {}'s proposal", p.walk),
                ..Default::default()
            };
            return Ok((
                vec![reused],
                Usage {
                    attempts: 1,
                    questions: 1,
                    ..Usage::default()
                },
            ));
        }
        let word: String = req
            .focus
            .as_ref()
            .map_or(req.state["goal"].as_str().unwrap_or("Other"), |f| {
                f.arrow.as_str()
            })
            .split_whitespace()
            .last()
            .unwrap_or("Other")
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect();
        let mut dst = word.clone();
        if let Some(first) = dst.get_mut(0..1) {
            first.make_ascii_uppercase();
        }
        let proposal = Proposal {
            arrow: format!("to_{}", word.to_lowercase()),
            src: req.at,
            dst,
            about: format!("cases about {word}"),
            rationale: format!("goal mentions `{word}`, which no arrow here covers"),
            ..Default::default()
        };
        Ok((
            vec![proposal],
            Usage {
                attempts: 1,
                questions: 1,
                ..Usage::default()
            },
        ))
    }
}
