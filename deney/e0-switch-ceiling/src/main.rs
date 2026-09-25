//! E0: the ceiling of a CPU switch learner, measured before building it.
//!
//! Every judged candidate in a recorded frame is one switch (one-vs-rest
//! for a choice, one per arrow for a noul, one per level for a score).
//! Each switch is described by the features `contracts/calibrated-switch.osil`
//! names, read from the record and from the category the run used. The
//! learner's capability admits some and refuses others; the admitted share
//! is the ceiling. Marginal and joint tables say what admitting each
//! refused feature would add.
//!
//! cargo run --release -- [--long 2000]   (from this folder)

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use onto_core::{Admission, Error, Module, ObjId};
use serde_json::{Value, json};

/// The refusals the capability declares, as measured here.
const REFUSALS: [&str; 3] = ["long_context", "open_vocabulary", "unlabelled"];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    let repo = here.join("../..").canonicalize()?;
    let args: Vec<String> = std::env::args().collect();
    let long = args
        .iter()
        .position(|a| a == "--long")
        .and_then(|i| args.get(i + 1))
        .map_or(2000, |v| v.parse().expect("--long N"));

    let manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(here.join("data/manifest.json"))?)?;
    let mut modules: HashMap<PathBuf, Module> = HashMap::new();
    let mut switches: Vec<Switch> = Vec::new();
    let mut per_run: Vec<Value> = Vec::new();
    let mut unresolved = 0usize;

    for run in manifest["runs"].as_array().unwrap() {
        let file = run["file"].as_str().unwrap();
        let specs: Vec<&str> = match run["columns"].as_array() {
            Some(cols) => cols.iter().map(|c| c.as_str().unwrap()).collect(),
            None => vec![run["spec"].as_str().unwrap()],
        };
        let mut cats = Vec::new();
        for spec in &specs {
            let (path, name) = split(spec);
            let full = repo.join(&path);
            if !modules.contains_key(&full) {
                modules.insert(full.clone(), load_module(&full)?);
            }
            let module = &modules[&full];
            let cat = match &name {
                Some(n) => module.category(n).cloned(),
                None => module.categories.first().cloned(),
            }
            .ok_or_else(|| format!("{spec}: no such category"))?;
            cats.push((cat, loops(module, &name)));
        }
        let text = std::fs::read_to_string(here.join("data").join(file))?;
        let before = switches.len();
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            let r: Value = serde_json::from_str(line)?;
            if r["judge"].is_null() {
                continue;
            }
            // Ensemble columns run with walk bases 1, 1_000_001, 2_000_001.
            let column = (r["walk"].as_u64().unwrap_or(1) / 1_000_000) as usize;
            let (cat, resonant) = &cats[column.min(cats.len() - 1)];
            let at = r["at"].as_str().unwrap_or_default();
            let object = cat.object_id(at).ok();
            if object.is_none() {
                unresolved += 1;
            }
            let seen_chars = r["seen"].to_string().chars().count();
            let open_frame = r["closure"] == "open"
                || object.is_none()
                || r["candidates"]
                    .as_array()
                    .is_some_and(|cs| cs.iter().any(|c| c["learned"] == true));
            let person = object.is_some_and(|o| cat.admission(o) != Admission::OpenWorld);
            for c in r["candidates"].as_array().into_iter().flatten() {
                if c["judgment"].is_null() {
                    continue;
                }
                let arrow = c["arrow"].as_str().unwrap_or_default();
                let evidence = cat
                    .arrow_id(arrow)
                    .is_ok_and(|a| cat.arrow(a).attested.is_some());
                let resonance = object.is_some_and(|o| resonant.contains(&o));
                let mut features = HashSet::new();
                if seen_chars > long {
                    features.insert("long_context");
                }
                if open_frame {
                    features.insert("open_vocabulary");
                }
                if !(evidence || person || resonance) {
                    features.insert("unlabelled");
                }
                switches.push(Switch {
                    run: file.to_owned(),
                    primitive: r["primitive"].as_str().unwrap_or_default().to_owned(),
                    seen_chars,
                    features,
                    sources: [
                        ("evidence", evidence),
                        ("person", person),
                        ("resonance", resonance),
                    ]
                    .into_iter()
                    .filter(|s| s.1)
                    .map(|s| s.0)
                    .collect(),
                });
            }
        }
        let mine = &switches[before..];
        per_run.push(json!({
            "run": file,
            "switches": mine.len(),
            "admitted": mine.iter().filter(|s| s.features.is_empty()).count(),
        }));
    }

    // Ceiling, marginal and joint tables.
    let n = switches.len();
    let share = |allowed: &[&str]| {
        switches
            .iter()
            .filter(|s| s.features.iter().all(|f| allowed.contains(f)))
            .count() as f64
            / n as f64
    };
    let ceiling = share(&[]);
    let mut joint = Vec::new();
    for mask in 0u32..(1 << REFUSALS.len()) {
        let allowed: Vec<&str> = (0..REFUSALS.len())
            .filter(|i| mask & (1 << i) != 0)
            .map(|i| REFUSALS[i])
            .collect();
        joint.push(json!({ "also_admitted": allowed, "ceiling": share(&allowed) }));
    }
    let blocked_by: BTreeMap<&str, usize> = REFUSALS
        .iter()
        .map(|f| (*f, switches.iter().filter(|s| s.features.contains(f)).count()))
        .collect();
    let mut sources: BTreeMap<String, usize> = BTreeMap::new();
    for s in &switches {
        let key = if s.sources.is_empty() {
            "none".to_owned()
        } else {
            s.sources.join("+")
        };
        *sources.entry(key).or_default() += 1;
    }
    let mut primitives: BTreeMap<&str, usize> = BTreeMap::new();
    for s in &switches {
        *primitives.entry(s.primitive.as_str()).or_default() += 1;
    }
    let mut lengths: Vec<usize> = switches.iter().map(|s| s.seen_chars).collect();
    lengths.sort_unstable();
    let pct = |p: f64| lengths[((lengths.len() - 1) as f64 * p) as usize];

    let out = json!({
        "long_context_chars": long,
        "switches": n,
        "unresolved_frames": unresolved,
        "ceiling": ceiling,
        "blocked_by": blocked_by,
        "joint": joint,
        "label_sources": sources,
        "primitives": primitives,
        "seen_chars": { "p50": pct(0.5), "p90": pct(0.9), "max": pct(1.0) },
        "runs": per_run,
    });
    let path = here.join(format!("results/ceiling-long{long}.json"));
    std::fs::write(&path, serde_json::to_string_pretty(&out)? + "\n")?;

    println!("E0 switch ceiling · {n} switches · long context > {long} chars");
    println!("  ceiling (admitted share): {:.1}%", 100.0 * ceiling);
    for (f, k) in &blocked_by {
        println!("  refused by {f:<16} {k:>4} ({:.1}%)", 100.0 * *k as f64 / n as f64);
    }
    println!("  joint: admitting also …");
    for j in &joint {
        println!(
            "    {:<48} {:.1}%",
            j["also_admitted"].to_string(),
            100.0 * j["ceiling"].as_f64().unwrap()
        );
    }
    println!("  label sources: {sources:?}");
    println!("  seen state chars: p50 {} · p90 {} · max {}", pct(0.5), pct(0.9), pct(1.0));
    println!("  unresolved frames (not in the declared category): {unresolved}");
    println!("  written {}", path.display());
    Ok(())
}

struct Switch {
    #[allow(dead_code)]
    run: String,
    primitive: String,
    seen_chars: usize,
    features: HashSet<&'static str>,
    sources: Vec<&'static str>,
}

/// Objects on a loop that must agree: in some functor's domain from this
/// category, or touched by an arrow on a path equation.
fn loops(module: &Module, name: &Option<String>) -> HashSet<ObjId> {
    let Some(cat) = (match name {
        Some(n) => module.category(n),
        None => module.categories.first(),
    }) else {
        return HashSet::new();
    };
    let mut out = HashSet::new();
    for f in module.functors.iter().filter(|f| f.src == cat.name()) {
        for (i, y) in f.objects.iter().enumerate() {
            if y.is_some() {
                out.insert(ObjId(i as u32));
            }
        }
    }
    for e in cat.equations() {
        for p in [&e.lhs, &e.rhs] {
            for a in &p.arrows {
                out.insert(cat.arrow(*a).src);
            }
        }
    }
    out
}

fn split(spec: &str) -> (PathBuf, Option<String>) {
    match spec.rsplit_once('#') {
        Some((f, n)) => (PathBuf::from(f), Some(n.to_owned())),
        None => (PathBuf::from(spec), None),
    }
}

/// A module with its imports, resolved relative to each importing file.
fn load_module(file: &Path) -> Result<Module, Box<dyn std::error::Error>> {
    let src = std::fs::read_to_string(file)?;
    let mut loaded: HashSet<PathBuf> = [file.to_path_buf()].into();
    let m = onto_core::parse_module_at(&file.to_string_lossy(), &src, &mut |path, importer| {
        let base = Path::new(importer).parent().unwrap_or(Path::new("."));
        let full = std::fs::canonicalize(base.join(path)).map_err(|e| Error::Parse {
            line: 0,
            msg: format!("import \"{path}\": {e}"),
        })?;
        let id = full.to_string_lossy().into_owned();
        if !loaded.insert(full.clone()) {
            return Ok((id, String::new()));
        }
        let text = std::fs::read_to_string(&full).map_err(|e| Error::Parse {
            line: 0,
            msg: format!("{}: {e}", full.display()),
        })?;
        Ok((id, text))
    })?;
    Ok(m)
}
