//! Local System-1 judges (`docs/11-platforms.md` §4, D71).
//!
//! The OpenJev method: a frame's state and question become one prompt
//! whose options are labelled `A`, `B`, …; one forward pass; the logits of
//! the label tokens at the answer position, normalized with a softmax, are
//! the option probabilities. No token is sampled.
//!
//! One model is loaded, on one worker thread that answers requests in
//! order; the engine keeps walking other cases meanwhile. Tokens already in
//! the model's cache (the instructions, a case's state) are not computed
//! again: questions over the same state pay only for their own tokens.

pub mod prompt;

#[cfg(feature = "llama")]
pub mod llama;

use std::sync::mpsc;

use onto_core::walk::Answer;
use onto_models::{Critic, FrameRequest, Judge, ModelError, NoulQuestion, Usage};
use serde_json::Value;
use tokio::sync::oneshot;

pub use prompt::Prompt;

/// What a scorer returns for one prompt: a probability per label, and how
/// many tokens it had to compute (the rest came from the cache).
pub struct Scored {
    pub probs: Vec<f32>,
    pub computed_tokens: u64,
}

/// Scores prompts: one probability vector per prompt, over its labels.
pub type Scorer<'a> = Box<dyn FnMut(&[Prompt]) -> Result<Vec<Scored>, String> + 'a>;

struct Job {
    prompts: Vec<Prompt>,
    reply: oneshot::Sender<Result<Vec<Scored>, String>>,
}

/// A judge and critic answering on this device, through a scorer that
/// owns the model on its own thread.
pub struct LocalJudge {
    name: String,
    jobs: Option<mpsc::Sender<Job>>,
    worker: Option<std::thread::JoinHandle<()>>,
}

/// Stops the worker and waits for it, so the model and its GPU buffers
/// are released before the process exits (Metal asserts otherwise).
impl Drop for LocalJudge {
    fn drop(&mut self) {
        self.jobs.take();
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
    }
}

impl LocalJudge {
    /// Starts the worker thread. `load` runs on it: it loads the model and
    /// hands a scorer (which may borrow the model) to `serve`, which
    /// answers requests until the judge is dropped.
    pub fn spawn<F>(name: String, load: F) -> Result<Self, String>
    where
        F: FnOnce(&mut dyn FnMut(Scorer<'_>)) -> Result<(), String> + Send + 'static,
    {
        let (tx, rx) = mpsc::channel::<Job>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();
        let worker = std::thread::Builder::new()
            .name("onto-local".into())
            .spawn(move || {
                let ready = ready_tx.clone();
                let mut serve = move |mut score: Scorer<'_>| {
                    let _ = ready.send(Ok(()));
                    while let Ok(job) = rx.recv() {
                        let _ = job.reply.send(score(&job.prompts));
                    }
                };
                if let Err(e) = load(&mut serve) {
                    let _ = ready_tx.send(Err(e));
                }
            })
            .map_err(|e| e.to_string())?;
        ready_rx
            .recv()
            .map_err(|_| "the local model thread stopped while loading".to_string())??;
        Ok(Self {
            name,
            jobs: Some(tx),
            worker: Some(worker),
        })
    }

    async fn ask(&self, prompts: Vec<Prompt>) -> Result<Vec<Scored>, ModelError> {
        let stopped = || ModelError::Http("the local model thread has stopped".into());
        let (reply, answer) = oneshot::channel();
        self.jobs
            .as_ref()
            .ok_or_else(stopped)?
            .send(Job { prompts, reply })
            .map_err(|_| stopped())?;
        answer
            .await
            .map_err(|_| stopped())?
            .map_err(ModelError::Decode)
    }
}

fn usage(scored: &[Scored]) -> Usage {
    Usage {
        input_tokens: scored.iter().map(|s| s.computed_tokens).sum(),
        output_tokens: 0,
        attempts: 1,
        questions: scored.len() as u32,
    }
}

impl Judge for LocalJudge {
    fn name(&self) -> String {
        format!("local:{}", self.name)
    }

    async fn judge(&self, req: FrameRequest) -> Result<(Answer, Usage), ModelError> {
        let prompts = prompt::frame(&req)?;
        let scored = self.ask(prompts).await?;
        let probs: Vec<&[f32]> = scored.iter().map(|s| s.probs.as_slice()).collect();
        let answer = prompt::read(&req, &probs)
            .ok_or_else(|| ModelError::Decode("the local readout does not fit the frame".into()))?;
        Ok((answer, usage(&scored)))
    }
}

impl Critic for LocalJudge {
    fn name(&self) -> String {
        Judge::name(self)
    }

    async fn nouls(
        &self,
        state: Value,
        questions: Vec<NoulQuestion>,
    ) -> Result<(Vec<f32>, Usage), ModelError> {
        let prompts = questions
            .iter()
            .map(|q| prompt::critic(&state, q))
            .collect();
        let scored = self.ask(prompts).await?;
        let yes = scored.iter().map(|s| s.probs[0]).collect();
        Ok((yes, usage(&scored)))
    }
}
