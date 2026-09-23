//! `onto migrate`: carry the learned layer and the memory file along a
//! version functor instead of retiring them.
//!
//! Learned arrows are renamed along the object map (objects the open world
//! created keep their names) and replayed against the new version through
//! the same proofs. Precedents move to the image frame and the image
//! arrow; a precedent recorded at a frame whose options changed answered
//! a different question and is dropped (`--keep-stale` keeps it).

use std::io::Write;
use std::path::PathBuf;

use clap::Args;
use onto_runtime::memory::Precedent;

use crate::run::BoxError;

#[derive(Args)]
pub struct MigrateArgs {
    /// `FILE#Functor`: a functor from the old version to the new one.
    functor: PathBuf,
    /// The old learned layer.
    #[arg(long, requires = "to_learned")]
    learned: Option<PathBuf>,
    #[arg(long)]
    to_learned: Option<PathBuf>,
    /// The old memory file.
    #[arg(long, requires = "to_memory")]
    memory: Option<PathBuf>,
    #[arg(long)]
    to_memory: Option<PathBuf>,
    /// Keep precedents recorded at frames whose options changed.
    #[arg(long)]
    keep_stale: bool,
}

pub fn main(args: MigrateArgs) -> Result<(), BoxError> {
    let (file, name) = crate::module::target(&args.functor);
    let (module, _) = crate::module::load_module(&file)?;
    let f = match &name {
        Some(n) => module.functor(n),
        None if module.functors.len() == 1 => module.functors.first(),
        None => None,
    }
    .ok_or_else(|| format!("{}: name one functor with #Name", file.display()))?;
    let a = module.category(&f.src).expect("loaded");
    let b = module.category(&f.dst).expect("loaded");
    let object = |n: &str| -> String {
        a.object_id(n)
            .ok()
            .and_then(|x| f.object(x))
            .map_or_else(|| n.to_owned(), |y| b.object(y).name.clone())
    };
    println!("migrate along {}: {} -> {}", f.name, f.src, f.dst);

    if let (Some(from), Some(to)) = (&args.learned, &args.to_learned) {
        let entries = crate::learned::read(from)?;
        let mut cat = b.clone();
        let mut out = std::fs::File::create(to)?;
        let (mut kept, mut retired) = (0, 0);
        for mut l in entries {
            l.proposal.src = object(&l.proposal.src);
            l.proposal.dst = object(&l.proposal.dst);
            let (next, skipped) = cat.clone().with_learned(std::slice::from_ref(&l.proposal));
            match skipped.first() {
                Some((_, why)) => {
                    retired += 1;
                    println!("  learned {}: retired ({why})", l.proposal.arrow);
                }
                None => {
                    cat = next;
                    kept += 1;
                    writeln!(out, "{}", serde_json::to_string(&l)?)?;
                }
            }
        }
        println!(
            "  learned layer: {kept} migrated, {retired} retired → {}",
            to.display()
        );
    }

    if let (Some(from), Some(to)) = (&args.memory, &args.to_memory) {
        let text = std::fs::read_to_string(from)?;
        let changed: Vec<&str> = f
            .report
            .changed_frames
            .iter()
            .filter_map(|c| c.split(':').next())
            .collect();
        let mut out = std::fs::File::create(to)?;
        let (mut kept, mut stale, mut lost) = (0, 0, 0);
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            let mut p: Precedent = serde_json::from_str(line)?;
            if changed.contains(&p.frame.as_str()) && !args.keep_stale {
                stale += 1;
                continue;
            }
            let image = a
                .arrow_id(&p.decided)
                .ok()
                .and_then(|x| f.arrows.get(x.0 as usize).cloned().flatten());
            let Some(img) = image.filter(|i| i.arrows.len() == 1) else {
                lost += 1;
                continue;
            };
            p.frame = object(&p.frame);
            p.decided = b.arrow(img.arrows[0]).name.clone();
            writeln!(out, "{}", serde_json::to_string(&p)?)?;
            kept += 1;
        }
        println!(
            "  memory: {kept} migrated, {stale} dropped as stale (their frame's options changed), {lost} without a single image option → {}",
            to.display()
        );
    }
    Ok(())
}
