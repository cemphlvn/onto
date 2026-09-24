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
use onto_runtime::engine::{ArrowState, Learned};
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
    /// `assured`: assured evolution at every learnable frame (learn only
    /// what every check passed; hold the undecided for a person).
    /// `policy` (default): each frame's declared admission. A run can
    /// tighten admission, never loosen it.
    #[arg(long = "loop", value_enum, default_value_t = Loop::Policy)]
    pub learning: Loop,
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
    /// Gap signals for curation (default: `<file stem>.gaps.jsonl`).
    #[arg(long)]
    pub gaps: Option<PathBuf>,
}

#[derive(Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Loop {
    Policy,
    Assured,
}

impl WorldArgs {
    /// Whether this run forces assured admission everywhere.
    pub fn assured(&self) -> bool {
        self.learning == Loop::Assured
    }

    /// The same world for one part of a multi-category run (a column of an
    /// ensemble): explicit files become `<stem>.<part>.<ext>`, so parts
    /// never share a learned layer, memory or gap queue.
    pub fn part(&self, part: &str) -> Self {
        let split = |p: &Option<PathBuf>| {
            p.as_ref().map(|p| {
                let ext = p
                    .extension()
                    .map_or(String::new(), |e| e.to_string_lossy().into_owned());
                p.with_extension(format!("{part}.{ext}"))
            })
        };
        Self {
            learned: split(&self.learned),
            memory: split(&self.memory),
            gaps: split(&self.gaps),
            ..self.clone()
        }
    }

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
        // Only active arrows are in the graph; dormant ones wait in the
        // library (`Engine::use_library`), retired ones are kept as history.
        let proposals: Vec<_> = entries
            .iter()
            .filter(|l| l.state.is_active())
            .map(|l| l.proposal.clone())
            .collect();
        let shelved = |s: ArrowState| entries.iter().filter(|l| l.state == s).count();
        let (dormant, retired) = (shelved(ArrowState::Dormant), shelved(ArrowState::Retired));
        let learning = if self.assured() {
            "assured admission everywhere".to_owned()
        } else {
            format!(
                "admission per policy (default {})",
                declared.default_admission().as_str()
            )
        };
        let (cat, skipped) = declared.with_learned(&proposals);
        println!(
            "world: open · {learning} · learned layer {} ({} arrows loaded{}{}{})",
            layer.display(),
            proposals.len() - skipped.len(),
            if skipped.is_empty() {
                String::new()
            } else {
                format!(", {} no longer hold", skipped.len())
            },
            if dormant > 0 {
                format!(", {dormant} dormant in the library")
            } else {
                String::new()
            },
            if retired > 0 {
                format!(", {retired} retired")
            } else {
                String::new()
            },
        );
        for (arrow, why) in skipped {
            println!("  no longer holds: {arrow}: {why}");
        }
        Ok(Arc::new(cat))
    }

    /// Functors from this category (declared in or imported by its file),
    /// for hierarchy and transport; and the learned layer's entries, so
    /// transported structure keeps its image.
    pub fn lenses(
        &self,
        file: &Path,
        cat: &Category,
    ) -> Result<(Vec<onto_runtime::lens::Lens>, Vec<Learned>), BoxError> {
        let (module, _) = crate::module::load_module(&crate::module::target(file).0)?;
        let lenses: Vec<onto_runtime::lens::Lens> = module
            .functors
            .iter()
            .filter(|f| f.src == cat.name())
            .map(|f| onto_runtime::lens::Lens {
                functor: f.clone(),
                target: Arc::new(module.category(&f.dst).expect("loaded").clone()),
            })
            .collect();
        for o in cat.objects() {
            if let Some(g) = &o.frame.grouped_by
                && !lenses.iter().any(|l| &l.functor.name == g)
            {
                println!(
                    "warning: frame {} is grouped by `{g}`, which is not a functor from {}; judged ungrouped",
                    o.name,
                    cat.name()
                );
            }
        }
        for l in &lenses {
            let transport = if l.functor.transport {
                " · transport"
            } else {
                ""
            };
            println!(
                "functor {}: {} -> {}{transport}",
                l.functor.name, l.functor.src, l.functor.dst
            );
        }
        let entries = if self.closed_world {
            Vec::new()
        } else {
            read(&self.layer(file))?
        };
        Ok((lenses, entries))
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

    /// Appends this run's gap signals (stops a person or curation acts on).
    pub fn save_gaps(
        &self,
        file: &Path,
        gaps: &[onto_runtime::curation::GapSignal],
    ) -> Result<(), BoxError> {
        if gaps.is_empty() {
            return Ok(());
        }
        let path = self
            .gaps
            .clone()
            .unwrap_or_else(|| sibling(file, "gaps.jsonl"));
        let mut out = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        for g in gaps {
            writeln!(out, "{}", serde_json::to_string(g)?)?;
        }
        println!(
            "gaps: {} signal(s) for curation appended to {} (onto curate)",
            gaps.len(),
            path.display()
        );
        Ok(())
    }

    /// Dormant arrows the library answered a gap with are active again: a
    /// recall is evidence the structure is needed.
    pub fn save_recalled(
        &self,
        file: &Path,
        recalled: &[(String, String)],
    ) -> Result<(), BoxError> {
        if self.closed_world || recalled.is_empty() {
            return Ok(());
        }
        let layer = self.layer(file);
        let mut entries = read(&layer)?;
        for (arrow, record) in recalled {
            if let Some(l) = entries
                .iter_mut()
                .find(|l| &l.proposal.arrow == arrow && l.state == ArrowState::Dormant)
            {
                l.state = ArrowState::Active;
                l.note = Some(format!("recalled from the library at {record}"));
                println!("library: recalled {arrow} at {record}; active again");
            }
        }
        write(&layer, &entries)
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

/// Rewrites the layer (state changes; arrows are never removed).
pub fn write(layer: &Path, entries: &[Learned]) -> Result<(), BoxError> {
    let mut out = String::new();
    for l in entries {
        out.push_str(&serde_json::to_string(l)?);
        out.push('\n');
    }
    std::fs::write(layer, out)?;
    Ok(())
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
    /// Put an arrow in the library: out of the graph, recallable at gaps.
    #[arg(long, value_name = "ARROW")]
    dormant: Vec<String>,
    /// Put an arrow back in the graph.
    #[arg(long, value_name = "ARROW")]
    activate: Vec<String>,
    /// Retire an arrow: kept as history, never in the graph or recalled.
    #[arg(long, value_name = "ARROW")]
    retire: Vec<String>,
    /// Why (recorded with the state change).
    #[arg(long)]
    note: Option<String>,
    /// Frame records of runs (`--dispositions`): how often each learned
    /// arrow was judged and taken; recommendations only.
    #[arg(long, value_delimiter = ',')]
    usage: Vec<PathBuf>,
    /// Judged at least this often and never taken: recommend dormancy.
    #[arg(long, default_value_t = 3)]
    min_judged: usize,
    #[command(flatten)]
    world: WorldArgs,
}

/// How a learned arrow fared in recorded runs.
#[derive(Default)]
struct Usage {
    judged: usize,
    taken: usize,
    judgment_sum: f64,
}

/// `onto learned FILE`: the library of learned structure, each arrow's
/// state, and whether it still holds against the declared graph. State
/// changes are a person's act (`--dormant`, `--activate`, `--retire`);
/// `--usage` only recommends.
pub fn main(args: LearnedArgs) -> Result<(), BoxError> {
    let layer = args.world.layer(&args.file);
    let mut entries = read(&layer)?;
    let changes = args
        .dormant
        .iter()
        .map(|a| (a, ArrowState::Dormant))
        .chain(args.activate.iter().map(|a| (a, ArrowState::Active)))
        .chain(args.retire.iter().map(|a| (a, ArrowState::Retired)));
    let mut changed = false;
    for (arrow, state) in changes {
        let l = entries
            .iter_mut()
            .find(|l| &l.proposal.arrow == arrow)
            .ok_or_else(|| format!("{}: no learned arrow `{arrow}`", layer.display()))?;
        println!("{arrow}: {} → {}", l.state.as_str(), state.as_str());
        l.state = state;
        l.note = Some(args.note.clone().unwrap_or_else(|| "by a person".into()));
        changed = true;
    }
    if changed {
        write(&layer, &entries)?;
        println!();
    }

    let mut usage: std::collections::HashMap<String, Usage> = Default::default();
    for path in &args.usage {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        for r in text
            .lines()
            .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        {
            let taken = r["outcome"]["arrow"].as_str();
            for c in r["candidates"].as_array().into_iter().flatten() {
                let (Some(a), Some(j)) = (c["arrow"].as_str(), c["judgment"].as_f64()) else {
                    continue;
                };
                let u = usage.entry(a.to_owned()).or_default();
                u.judged += 1;
                u.judgment_sum += j;
                if taken == Some(a) {
                    u.taken += 1;
                }
            }
        }
    }

    let mut cat = crate::module::load_category(&args.file)?;
    println!("{} ({} arrows)", layer.display(), entries.len());
    let mut advice = Vec::new();
    for l in &entries {
        let p = &l.proposal;
        let holds = if l.state.is_active() {
            let (next, skipped) = cat.clone().with_learned(std::slice::from_ref(p));
            match skipped.first() {
                Some((_, why)) => format!("no longer holds: {why}"),
                None => {
                    cat = next;
                    "holds".into()
                }
            }
        } else {
            "in the library".into()
        };
        println!();
        println!(
            "  {}: {} -> {}   [{} · {holds}]",
            p.arrow,
            p.src,
            p.dst,
            l.state.as_str()
        );
        if !p.about.is_empty() {
            println!("    about: {}", p.about);
        }
        println!("    learned at {} ({})", l.record, l.reason);
        if let Some(n) = &l.note {
            println!("    note: {n}");
        }
        println!("    checks: {}", l.checks.join(", "));
        if !args.usage.is_empty() {
            match usage.get(&p.arrow) {
                Some(u) => {
                    let mean = u.judgment_sum / u.judged as f64;
                    println!(
                        "    usage: judged {} time(s), taken {}, mean judgment {mean:.2}",
                        u.judged, u.taken
                    );
                    if l.state.is_active() && u.taken == 0 && u.judged >= args.min_judged {
                        advice.push(format!(
                            "{a}: present in {} judgments at {} and never taken (mean judgment {mean:.2}). \
                             Before `--dormant {a}`: its presence effect (`onto replay … --without-arrow {a}`), \
                             and whether these runs had a case that needs it at all. A dormant arrow is \
                             recalled only at a gap, and other options at {} may absorb such a case without \
                             one: audit later records with `onto replay … --with-arrow {a}`",
                            u.judged, p.src, p.src, a = p.arrow
                        ));
                    }
                }
                None => println!("    usage: not reached in these runs"),
            }
            if l.state == ArrowState::Dormant {
                advice.push(format!(
                    "{a} is dormant: check that {} still escalates the cases it covered \
                     (`onto replay … --with-arrow {a}` on records at {}); if they are taken \
                     elsewhere, `--activate {a}`",
                    p.src,
                    p.src,
                    a = p.arrow
                ));
            }
        }
    }
    if !advice.is_empty() {
        println!();
        println!("recommendations (nothing is changed automatically):");
        for a in advice {
            println!("  - {a}");
        }
    }
    Ok(())
}
