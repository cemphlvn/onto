//! The llama.cpp scorer (llama-cpp-2): one GGUF model, one context, a
//! token cache shared by consecutive prompts. Builds for macOS and iOS
//! (Metal), Linux and Windows (CPU, CUDA, Vulkan) and Android.

use std::num::NonZeroU32;
use std::path::PathBuf;

use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::context::params::LlamaContextParams;
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
            .with_n_ubatch(512);
        let ctx = model
            .new_context(&backend, ctx_params)
            .map_err(|e| e.to_string())?;
        let mut session = Session::new(&model, ctx, opts.n_ctx as usize)?;
        serve(Box::new(move |prompts| session.score(prompts)));
        Ok(())
    })
}

struct Session<'m> {
    model: &'m LlamaModel,
    ctx: LlamaContext<'m>,
    /// Tokens whose keys and values are in the cache, in order.
    cached: Vec<LlamaToken>,
    batch: LlamaBatch<'static>,
    /// Appended after the chat template: an empty reasoning block for
    /// models that think first (Qwen3), then the answer's lead-in.
    answer_lead: String,
    template: llama_cpp_2::model::LlamaChatTemplate,
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
            batch: LlamaBatch::new(n_ctx, 1),
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

    /// The answer lead ends in `Answer:`; the letter comes next with its
    /// leading space, as one token (` A`) in BPE vocabularies. Reading a
    /// bare `A` after a lone space token biased small models toward one
    /// letter (MiniCPM5-1B chose `E` for every ticket).
    fn label_tokens(&self, labels: &[String]) -> Result<Vec<LlamaToken>, String> {
        let one = |s: &str| match self.model.str_to_token(s, AddBos::Never) {
            Ok(t) if t.len() == 1 => Some(t[0]),
            _ => None,
        };
        labels
            .iter()
            .map(|l| {
                one(&format!(" {l}"))
                    .or_else(|| one(l))
                    .ok_or_else(|| format!("label `{l}` is not one token in this vocabulary"))
            })
            .collect()
    }

    /// Makes the cache hold exactly `tokens` (reusing their longest cached
    /// prefix) and returns the logits after the last one, plus how many
    /// tokens were computed.
    fn eval(&mut self, tokens: &[LlamaToken]) -> Result<(Vec<f32>, u64), String> {
        let n_ctx = self.ctx.n_ctx() as usize;
        if tokens.len() >= n_ctx {
            return Err(format!(
                "prompt of {} tokens exceeds the context ({n_ctx})",
                tokens.len()
            ));
        }
        let mut keep = self
            .cached
            .iter()
            .zip(tokens)
            .take_while(|(a, b)| a == b)
            .count();
        // The last token is always evaluated: its logits are the answer.
        keep = keep.min(tokens.len() - 1);
        let cut = self
            .ctx
            .clear_kv_cache_seq(Some(0), Some(keep as u32), None)
            .map_err(|e| e.to_string())?;
        if !cut {
            // Recurrent and hybrid models (Qwen3.5's linear-attention
            // layers) cannot drop a suffix of their state: start over.
            self.ctx.clear_kv_cache();
            keep = 0;
        }
        self.cached.truncate(keep);

        let fresh = &tokens[keep..];
        self.batch.clear();
        for (i, t) in fresh.iter().enumerate() {
            let last = i + 1 == fresh.len();
            self.batch
                .add(*t, (keep + i) as i32, &[0], last)
                .map_err(|e| e.to_string())?;
        }
        self.ctx
            .decode(&mut self.batch)
            .map_err(|e| e.to_string())?;
        self.cached.extend_from_slice(fresh);
        let logits = self.ctx.get_logits_ith(self.batch.n_tokens() - 1).to_vec();
        Ok((logits, fresh.len() as u64))
    }

    fn score(&mut self, prompts: &[Prompt]) -> Result<Vec<Scored>, String> {
        prompts
            .iter()
            .map(|p| {
                let tokens = self.tokens(p)?;
                let labels = self.label_tokens(&p.labels)?;
                let (logits, computed_tokens) = self.eval(&tokens)?;
                let chosen: Vec<f32> = labels.iter().map(|t| logits[t.0 as usize]).collect();
                Ok(Scored {
                    probs: softmax(&chosen),
                    computed_tokens,
                })
            })
            .collect()
    }
}

fn softmax(xs: &[f32]) -> Vec<f32> {
    let max = xs.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let exps: Vec<f32> = xs.iter().map(|x| (x - max).exp()).collect();
    let sum: f32 = exps.iter().sum();
    exps.iter().map(|e| e / sum).collect()
}
