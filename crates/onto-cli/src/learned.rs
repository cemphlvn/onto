//! The learned layer: structure the open world admitted, kept apart from
//! the declared policy.
//!
//! `<stem>.learned.jsonl` sits next to `<stem>.onto`, one admitted arrow
//! per line. The declared file is never written by a run. Every load
//! replays the layer through the same structural proofs (well-formedness,
//! capability authority, invariants) against the *current* declared graph,
//! so a policy change can retire learned structure but learned structure
//! can never change policy. `--closed-world` ignores the layer.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use clap::Args;
use onto_core::Category;
use onto_runtime::engine::Learned;
use onto_runtime::memory::Precedent;

use crate::run::BoxError;

#[derive(Args, Clone)]
pub struct WorldArgs {
    /// Declared graph only: at a missing enumeration the walk stops and
    /// System 2's proposals wait for a person (`onto review`/`promote`).
    /// Without it (open world), proposals the supervisor admits are
    /// learned and the walk continues.
    #[arg(long)]
    pub closed_world: bool,
    /// The learned layer (default: `<file stem>.learned.jsonl`).
    #[arg(long)]
    pub learned: Option<PathBuf>,
    /// Frames one walk may extend (open world).
    #[arg(long, default_value_t = 3)]
    pub max_expansions: usize,
    /// Precedents for frames that declare `memory: similar N` (default:
    /// `<file stem>.memory.jsonl`).
    #[arg(long)]
    pub memory: Option<PathBuf>,
    /// Neither load nor record precedents.
    #[arg(long)]
    pub no_memory: bool,
}

impl WorldArgs {
    pub fn layer(&self, file: &Path) -> PathBuf {
        self.learned
            .clone()
            .unwrap_or_else(|| sibling(file, "learned.jsonl"))
    }

    /// The declared graph, plus the learned layer in open world.
    pub fn load(&self, file: &Path) -> Result<Arc<Category>, BoxError> {
        let declared = crate::module::load_category(file)?;
        if self.closed_world {
            println!("world: closed (declared graph only)");
            return Ok(Arc::new(declared));
        }
        let layer = self.layer(file);
        let entries = read(&layer)?;
        let proposals: Vec<_> = entries.iter().map(|l| l.proposal.clone()).collect();
        let (cat, skipped) = declared.with_learned(&proposals);
        println!(
            "world: open · learned layer {} ({} arrows loaded{})",
            layer.display(),
            proposals.len() - skipped.len(),
            if skipped.is_empty() {
                String::new()
            } else {
                format!(", {} retired", skipped.len())
            }
        );
        for (arrow, why) in skipped {
            println!("  retired {arrow}: {why}");
        }
        Ok(Arc::new(cat))
    }

    fn memory_file(&self, file: &Path) -> PathBuf {
        self.memory
            .clone()
            .unwrap_or_else(|| sibling(file, "memory.jsonl"))
    }

    /// Precedents from earlier runs, if the category declares memory.
    pub fn precedents(&self, file: &Path, cat: &Category) -> Result<Vec<Precedent>, BoxError> {
        let declared = (0..cat.objects().len() as u32)
            .any(|o| cat.state_of(onto_core::ObjId(o)).memory.is_some());
        if self.no_memory || !declared {
            return Ok(Vec::new());
        }
        let path = self.memory_file(file);
        let Ok(text) = std::fs::read_to_string(&path) else {
            println!("memory: {} (empty)", path.display());
            return Ok(Vec::new());
        };
        let ps: Vec<Precedent> = text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(serde_json::from_str)
            .collect::<Result<_, _>>()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        println!("memory: {} ({} precedents)", path.display(), ps.len());
        Ok(ps)
    }

    /// Appends this run's decisions at memory frames.
    pub fn save_precedents(&self, file: &Path, ps: &[Precedent]) -> Result<(), BoxError> {
        if self.no_memory || ps.is_empty() {
            return Ok(());
        }
        let path = self.memory_file(file);
        let mut out = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        for p in ps {
            writeln!(out, "{}", serde_json::to_string(p)?)?;
        }
        println!(
            "memory: {} precedent(s) appended to {}",
            ps.len(),
            path.display()
        );
        Ok(())
    }

    /// Appends newly learned arrows to the layer.
    pub fn save(&self, file: &Path, learned: &[Learned]) -> Result<(), BoxError> {
        if self.closed_world || learned.is_empty() {
            return Ok(());
        }
        let layer = self.layer(file);
        let mut out = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&layer)?;
        for l in learned {
            writeln!(out, "{}", serde_json::to_string(l)?)?;
        }
        println!(
            "learned: {} arrow(s) appended to {}",
            learned.len(),
            layer.display()
        );
        Ok(())
    }
}

/// `x.onto` → `x.<ext>`; `x.onto#Name` → `x.Name.<ext>`.
fn sibling(spec: &Path, ext: &str) -> PathBuf {
    let (file, name) = crate::module::target(spec);
    match name {
        Some(n) => file.with_extension(format!("{n}.{ext}")),
        None => file.with_extension(ext),
    }
}

pub fn read(layer: &Path) -> Result<Vec<Learned>, BoxError> {
    let Ok(text) = std::fs::read_to_string(layer) else {
        return Ok(Vec::new());
    };
    text.lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
        .map(|(i, l)| {
            serde_json::from_str(l)
                .map_err(|e| format!("{}:{}: {e}", layer.display(), i + 1).into())
        })
        .collect()
}

#[derive(Args)]
pub struct LearnedArgs {
    file: PathBuf,
    #[command(flatten)]
    world: WorldArgs,
}

/// `onto learned FILE`: the learned layer, and whether each arrow still
/// holds against the declared graph.
pub fn main(args: LearnedArgs) -> Result<(), BoxError> {
    let layer = args.world.layer(&args.file);
    let entries = read(&layer)?;
    let mut cat = crate::module::load_category(&args.file)?;
    println!("{} ({} arrows)", layer.display(), entries.len());
    for l in &entries {
        let p = &l.proposal;
        let (next, skipped) = cat.clone().with_learned(std::slice::from_ref(p));
        let status = match skipped.first() {
            Some((_, why)) => format!("RETIRED ({why})"),
            None => {
                cat = next;
                "holds".into()
            }
        };
        println!();
        println!("  {}: {} -> {}   [{status}]", p.arrow, p.src, p.dst);
        if !p.about.is_empty() {
            println!("    about: {}", p.about);
        }
        println!("    learned at {} ({})", l.record, l.reason);
        println!("    checks: {}", l.checks.join(", "));
    }
    Ok(())
}
