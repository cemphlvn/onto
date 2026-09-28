//! The llama.cpp scorer (llama-cpp-2): one GGUF model, one context, a
//! token cache shared by consecutive prompts. Builds for macOS and iOS
//! (Metal), Linux and Windows (CPU, CUDA, Vulkan) and Android.

use std::num::NonZeroU32;
use std::path::PathBuf;

use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::context::session::{LlamaStateSeqFlags, SeqState};
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaChatMessage, LlamaModel};
use llama_cpp_2::token::LlamaToken;

use crate::{LocalJudge, Prompt, Scored};

#[derive(Clone, Debug)]
pub struct LlamaOptions {
    pub model: PathBuf,
    /// Context length in tokens: the longest prompt plus room for reuse.
    pub n_ctx: u32,
    /// Layers offloaded to the GPU (Metal, CUDA, Vulkan); 0 for CPU only.
    pub gpu_layers: u32,
}

impl LlamaOptions {
    pub fn new(model: impl Into<PathBuf>) -> Self {
        Self {
            model: model.into(),
            n_ctx: 8192,
            gpu_layers: 999,
        }
    }
}

/// Loads the model on the judge's worker thread and serves prompts.
pub fn judge(opts: LlamaOptions) -> Result<LocalJudge, String> {
    let name = opts
        .model
        .file_stem()
        .map_or("gguf".into(), |s| s.to_string_lossy().into_owned());
    LocalJudge::spawn(name, move |serve| {
        let mut backend = LlamaBackend::init().map_err(|e| e.to_string())?;
        backend.void_logs();
        let params = LlamaModelParams::default().with_n_gpu_layers(opts.gpu_layers);
        let model = LlamaModel::load_from_file(&backend, &opts.model, &params)
            .map_err(|e| format!("{}: {e}", opts.model.display()))?;
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(opts.n_ctx))
            .with_n_batch(opts.n_ctx)
            .with_n_ubatch(512)
            // Option suffixes run as parallel sequences sharing the prompt.
            .with_n_seq_max(SUFFIX_CHUNK as u32 + 1)
            .with_kv_unified(true);
        let ctx = model
            .new_context(&backend, ctx_params)
            .map_err(|e| e.to_string())?;
        let mut session = Session::new(&model, ctx, opts.n_ctx as usize)?;
        serve(Box::new(move |prompts| session.score(prompts)));
        Ok(())
    })
}

/// A snapshot of the model's state after `tokens`, for caches that cannot
/// drop a suffix (recurrent and hybrid models): restoring one replaces
/// recomputing its tokens.
struct Checkpoint {
    tokens: Vec<LlamaToken>,
    state: SeqState,
}

const CHECKPOINTS: usize = 4;
/// Option suffixes decoded together (each its own sequence).
const SUFFIX_CHUNK: usize = 32;

struct Session<'m> {
    model: &'m LlamaModel,
    ctx: LlamaContext<'m>,
    /// Tokens whose keys and values are in the cache, in order.
    cached: Vec<LlamaToken>,
    /// The previous prompt's tokens: what it shares with the next one is
    /// where a checkpoint pays off.
    last: Vec<LlamaToken>,
    /// Set once the cache refused to drop a suffix.
    rigid: bool,
    checkpoints: Vec<Checkpoint>,
    /// Content-free baseline scores per frame prompt (calibration).
    baselines: std::collections::HashMap<String, Vec<f32>>,
    batch: LlamaBatch<'static>,
    /// Appended after the chat template: an empty reasoning block for
    /// models that think first (Qwen3), then the answer's lead-in.
    answer_lead: String,
    template: llama_cpp_2::model::LlamaChatTemplate,
}

fn common(a: &[LlamaToken], b: &[LlamaToken]) -> usize {
    a.iter().zip(b).take_while(|(x, y)| x == y).count()
}

impl<'m> Session<'m> {
    fn new(model: &'m LlamaModel, ctx: LlamaContext<'m>, n_ctx: usize) -> Result<Self, String> {
        let template = model.chat_template(None).map_err(|e| e.to_string())?;
        let thinks = template.to_str().is_ok_and(|t| t.contains("<think>"));
        let answer_lead = if thinks {
            "<think>\n\n</think>\n\nAnswer:".into()
        } else {
            "Answer:".into()
        };
        Ok(Self {
            model,
            ctx,
            cached: Vec::new(),
            last: Vec::new(),
            rigid: false,
            checkpoints: Vec::new(),
            baselines: std::collections::HashMap::new(),
            batch: LlamaBatch::new(n_ctx, SUFFIX_CHUNK as i32 + 1),
            answer_lead,
            template,
        })
    }

    fn tokens(&self, p: &Prompt) -> Result<Vec<LlamaToken>, String> {
        let chat = [
            LlamaChatMessage::new("system".into(), p.system.clone()).map_err(|e| e.to_string())?,
            LlamaChatMessage::new("user".into(), p.user.clone()).map_err(|e| e.to_string())?,
        ];
        let mut text = self
            .model
            .apply_chat_template(&self.template, &chat, true)
            .map_err(|e| e.to_string())?;
        text.push_str(&self.answer_lead);
        if std::env::var_os("ONTO_LOCAL_DEBUG").is_some() {
            eprintln!("--- onto-local prompt ---\n{text}\n--- end ---");
        }
        self.model
            .str_to_token(&text, AddBos::Always)
            .map_err(|e| e.to_string())
    }

    fn one_token(&self, s: &str) -> Option<LlamaToken> {
        match self.model.str_to_token(s, AddBos::Never) {
            Ok(t) if t.len() == 1 => Some(t[0]),
            _ => None,
        }
    }

    /// The token for character `c` of a label at position `pos`. The
    /// answer lead ends in `Answer:`, so the first character comes with its
    /// leading space (` A`, one token in BPE vocabularies) when the
    /// vocabulary has it for every first character; later characters are
    /// bare. Reading a bare `A` after a lone space token biased small
    /// models toward one letter (MiniCPM5-1B chose `E` for every ticket).
    fn char_token(&self, c: char, pos: usize, spaced: bool) -> Result<LlamaToken, String> {
        let s = if pos == 0 && spaced {
            format!(" {c}")
        } else {
            c.to_string()
        };
        self.one_token(&s)
            .ok_or_else(|| format!("`{s}` is not one token in this vocabulary"))
    }

    fn decode(&mut self, fresh: &[LlamaToken], start: usize) -> Result<(), String> {
        self.batch.clear();
        for (i, t) in fresh.iter().enumerate() {
            let last = i + 1 == fresh.len();
            self.batch
                .add(*t, (start + i) as i32, &[0], last)
                .map_err(|e| e.to_string())?;
        }
        self.ctx
            .decode(&mut self.batch)
            .map_err(|e| e.to_string())?;
        self.cached.extend_from_slice(fresh);
        Ok(())
    }

    /// Snapshots the current cache (rigid caches only).
    fn checkpoint(&mut self) -> Result<(), String> {
        if !self.rigid || self.checkpoints.iter().any(|c| c.tokens == self.cached) {
            return Ok(());
        }
        let state = self
            .ctx
            .state_seq_get(0, LlamaStateSeqFlags::empty())
            .map_err(|e| e.to_string())?;
        if self.checkpoints.len() == CHECKPOINTS {
            self.checkpoints.remove(0);
        }
        self.checkpoints.push(Checkpoint {
            tokens: self.cached.clone(),
            state,
        });
        Ok(())
    }

    /// Makes the cache a prefix of `tokens` of at most `limit` tokens,
    /// reusing what it holds (cutting, or restoring a checkpoint on a rigid
    /// cache); returns its length. For rigid caches it first snapshots the
    /// point this prompt shares with the previous one.
    fn prepare(&mut self, tokens: &[LlamaToken], limit: usize) -> Result<usize, String> {
        let n_ctx = self.ctx.n_ctx() as usize;
        if tokens.len() >= n_ctx {
            return Err(format!(
                "prompt of {} tokens exceeds the context ({n_ctx})",
                tokens.len()
            ));
        }
        let mut keep = common(&self.cached, tokens).min(limit);
        if keep < self.cached.len() {
            let cut = !self.rigid
                && self
                    .ctx
                    .clear_kv_cache_seq(Some(0), Some(keep as u32), None)
                    .map_err(|e| e.to_string())?;
            if cut {
                self.cached.truncate(keep);
            } else {
                // Recurrent and hybrid models (Qwen3.5's linear-attention
                // layers) cannot drop a suffix: restore the longest
                // checkpoint this prompt extends, else start over.
                self.rigid = true;
                self.ctx.clear_kv_cache();
                self.cached.clear();
                keep = 0;
                let best = self
                    .checkpoints
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| c.tokens.len() <= limit && tokens.starts_with(&c.tokens))
                    .max_by_key(|(_, c)| c.tokens.len())
                    .map(|(i, _)| i);
                if let Some(i) = best {
                    let cp = self.checkpoints.remove(i);
                    self.ctx
                        .state_seq_set(&cp.state, 0)
                        .map_err(|e| e.to_string())?;
                    self.cached = cp.tokens.clone();
                    keep = self.cached.len();
                    self.checkpoints.push(cp); // most recently used last
                }
            }
        }
        let shared = common(&self.last, tokens).min(limit);
        if self.rigid && shared > keep {
            let fresh = tokens[keep..shared].to_vec();
            self.decode(&fresh, keep)?;
            keep = shared;
            self.checkpoint()?;
        }
        Ok(keep)
    }

    /// Makes the cache hold exactly `tokens` and returns the logits after
    /// the last one, plus how many tokens were computed.
    fn eval(&mut self, tokens: &[LlamaToken]) -> Result<(Vec<f32>, u64), String> {
        let before = self.cached.len();
        // The last token is always evaluated: its logits are the answer.
        let keep = self.prepare(tokens, tokens.len() - 1)?;
        let reused = before.min(keep);
        let fresh = tokens[keep..].to_vec();
        self.decode(&fresh, keep)?;
        self.last = tokens.to_vec();
        let logits = self.ctx.get_logits_ith(self.batch.n_tokens() - 1).to_vec();
        Ok((logits, (tokens.len() - reused) as u64))
    }

    /// Letters: one forward pass; the softmax of the label tokens' logits.
    fn read_letters(&mut self, prompt: &[LlamaToken], labels: &[String]) -> Result<Scored, String> {
        let spaced = labels
            .iter()
            .all(|l| self.one_token(&format!(" {l}")).is_some());
        let mut toks = prompt.to_vec();
        if !spaced {
            // No ` A` in this vocabulary: a space token, then the letter.
            toks.extend(self.one_token(" "));
        }
        let (logits, computed) = self.eval(&toks)?;
        let chosen = labels
            .iter()
            .map(|l| {
                let c = l.chars().next().ok_or("empty label")?;
                Ok(logits[self.char_token(c, 0, spaced)?.0 as usize])
            })
            .collect::<Result<Vec<f32>, String>>()?;
        Ok(Scored {
            probs: softmax(&chosen),
            computed_tokens: computed,
        })
    }

    /// Likelihood: each option's own text, then the end of the answer, is
    /// scored as the whole answer, log P(text, end | prompt), minus the
    /// same under the content-free baseline prompt (computed once per
    /// frame and kept); the softmax over options is the distribution.
    fn read_answers(
        &mut self,
        p: &Prompt,
        prompt: &[LlamaToken],
        answers: &[String],
    ) -> Result<Scored, String> {
        let suffixes = answers
            .iter()
            .map(|a| {
                let mut t = self
                    .model
                    .str_to_token(&format!(" {a}"), AddBos::Never)
                    .map_err(|e| e.to_string())?;
                t.push(self.model.token_eos());
                Ok(t)
            })
            .collect::<Result<Vec<_>, String>>()?;
        let (mut scores, mut computed) = self.logprobs(prompt, &suffixes)?;
        if let Some(b) = &p.baseline {
            if !self.baselines.contains_key(b) {
                let base = Prompt {
                    user: b.clone(),
                    ..p.clone()
                };
                let bt = self.tokens(&base)?;
                let (lp, c) = self.logprobs(&bt, &suffixes)?;
                computed += c;
                self.baselines.insert(b.clone(), lp);
            }
            for (s, b) in scores.iter_mut().zip(&self.baselines[b]) {
                *s -= b;
            }
        }
        let probs = softmax(&scores);
        if std::env::var_os("ONTO_LOCAL_DEBUG").is_some() {
            let mut top: Vec<(usize, f32)> = probs.iter().copied().enumerate().collect();
            top.sort_by(|a, b| b.1.total_cmp(&a.1));
            let top: Vec<String> = top
                .iter()
                .take(3)
                .map(|(i, p)| format!("{:?}={p:.3}", answers[*i]))
                .collect();
            eprintln!("onto-local: top {} (computed {computed})", top.join(" "));
        }
        Ok(Scored {
            probs,
            computed_tokens: computed,
        })
    }

    /// log P(suffix | prompt) for each suffix: the prompt minus its last
    /// token in sequence 0 (cut back to, or restored from a checkpoint on a
    /// rigid cache), then the suffixes as parallel sequences copied from
    /// it, a chunk per decode, each started by the prompt's last token.
    /// Sequences are only ever cleared whole, which recurrent and hybrid
    /// caches allow.
    fn logprobs(
        &mut self,
        prompt: &[LlamaToken],
        suffixes: &[Vec<LlamaToken>],
    ) -> Result<(Vec<f32>, u64), String> {
        let from = prompt.len() - 1;
        let keep = self.prepare(prompt, from)?;
        let mut computed = (from - keep) as u64;
        if keep < from {
            let fresh = prompt[keep..from].to_vec();
            self.decode(&fresh, keep)?;
        }
        self.last = prompt[..from].to_vec();
        self.checkpoint()?;
        let mut out = vec![0.0f32; suffixes.len()];
        for chunk_start in (0..suffixes.len()).step_by(SUFFIX_CHUNK) {
            let chunk = chunk_start..(chunk_start + SUFFIX_CHUNK).min(suffixes.len());
            self.batch.clear();
            let mut rows = Vec::new(); // (option, batch row, token that row predicts)
            for (j, i) in chunk.clone().enumerate() {
                let seq = (j + 1) as i32;
                self.ctx
                    .clear_kv_cache_seq(Some(seq as u32), None, None)
                    .map_err(|e| e.to_string())?;
                self.ctx
                    .copy_kv_cache_seq(0, seq, None, None)
                    .map_err(|e| e.to_string())?;
                let mut toks = vec![prompt[from]];
                toks.extend_from_slice(&suffixes[i][..suffixes[i].len() - 1]);
                for (k, t) in toks.iter().enumerate() {
                    rows.push((i, self.batch.n_tokens(), suffixes[i][k]));
                    self.batch
                        .add(*t, (from + k) as i32, &[seq], true)
                        .map_err(|e| e.to_string())?;
                }
                computed += toks.len() as u64;
            }
            self.ctx
                .decode(&mut self.batch)
                .map_err(|e| e.to_string())?;
            for (i, row, next) in rows {
                out[i] += log_softmax_at(self.ctx.get_logits_ith(row), next.0 as usize);
            }
            for j in 0..chunk.len() {
                self.ctx
                    .clear_kv_cache_seq(Some((j + 1) as u32), None, None)
                    .map_err(|e| e.to_string())?;
            }
        }
        Ok((out, computed))
    }

    fn score(&mut self, prompts: &[Prompt]) -> Result<Vec<Scored>, String> {
        prompts
            .iter()
            .map(|p| {
                let tokens = self.tokens(p)?;
                match &p.answers {
                    Some(answers) => self.read_answers(p, &tokens, answers),
                    None => self.read_letters(&tokens, &p.labels),
                }
            })
            .collect()
    }
}

fn log_softmax_at(row: &[f32], i: usize) -> f32 {
    let max = row.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let sum: f32 = row.iter().map(|x| (x - max).exp()).sum();
    row[i] - max - sum.ln()
}

fn softmax(xs: &[f32]) -> Vec<f32> {
    let max = xs.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let exps: Vec<f32> = xs.iter().map(|x| (x - max).exp()).collect();
    let sum: f32 = exps.iter().sum();
    exps.iter().map(|e| e / sum).collect()
}
