//! The behavioural quotient (diagnostics only): which objects no future
//! observation can tell apart.
//!
//! Every object's static profile is its frame primitive, closure, entry
//! contract, join policy, and the **set** of (arrow label, target) pairs
//! leaving it (`docs/02-dispositional-semantics.md` §1). Arrow names are not
//! part of a label; exact duplicates (same label, same target) collapse
//! under set semantics. Partition refinement starts from these local
//! profiles and refines by the classes of arrow targets until stable: the
//! result is the coarsest bisimulation.
//!
//! Two modes:
//!
//! - [`Mode::Exact`]: labels include descriptions (instructions, `about`).
//!   Classes with several objects are genuine duplicates.
//! - [`Mode::Structure`]: descriptions are ignored; preconditions, effects,
//!   levels, contracts and joins still count. Objects that fall together
//!   here but not in exact mode are told apart **only by their
//!   descriptions**: their identity is qualitative, not structural
//!   (the asymmetry check).
//!
//! Nothing here merges anything; the quotient is never applied to the graph.

use std::collections::{BTreeMap, BTreeSet};

use crate::category::{ArrowId, Category, ObjId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Exact,
    Structure,
}

/// Equivalence classes of objects, each sorted, classes in order of their
/// first member.
pub fn partition(cat: &Category, mode: Mode) -> Vec<Vec<ObjId>> {
    let n = cat.objects().len();
    let local: Vec<String> = (0..n as u32)
        .map(|o| local_profile(cat, ObjId(o), mode))
        .collect();

    // Initial partition: identical local profiles.
    let mut class: Vec<usize> = renumber(&local);
    loop {
        let signatures: Vec<String> = (0..n)
            .map(|o| {
                let arrows: BTreeSet<String> = cat
                    .out(ObjId(o as u32))
                    .iter()
                    .map(|a| {
                        format!(
                            "{}→{}",
                            label(cat, *a, mode),
                            class[cat.arrow(*a).dst.0 as usize]
                        )
                    })
                    .collect();
                format!("{}|{}|{:?}", class[o], local[o], arrows)
            })
            .collect();
        let next = renumber(&signatures);
        if next == class {
            break;
        }
        class = next;
    }

    let mut groups: BTreeMap<usize, Vec<ObjId>> = BTreeMap::new();
    for (o, c) in class.iter().enumerate() {
        groups.entry(*c).or_default().push(ObjId(o as u32));
    }
    let mut out: Vec<Vec<ObjId>> = groups.into_values().collect();
    out.sort_by_key(|g| g[0]);
    out
}

/// Arrows that add nothing under set semantics: same source, same label,
/// same target as an earlier arrow.
pub fn redundant_arrows(cat: &Category) -> Vec<(ArrowId, ArrowId)> {
    let mut first: BTreeMap<(ObjId, String, ObjId), ArrowId> = BTreeMap::new();
    let mut out = Vec::new();
    for i in 0..cat.arrows().len() as u32 {
        let a = ArrowId(i);
        let x = cat.arrow(a);
        let key = (x.src, label(cat, a, Mode::Exact), x.dst);
        match first.get(&key) {
            Some(&earlier) => out.push((earlier, a)),
            None => {
                first.insert(key, a);
            }
        }
    }
    out
}

/// Objects distinguished **only by descriptions**: in a shared class under
/// [`Mode::Structure`], alone under [`Mode::Exact`]. Returned as the
/// structural classes they fall into.
pub fn label_only_identities(cat: &Category) -> Vec<Vec<ObjId>> {
    let exact = partition(cat, Mode::Exact);
    let singleton = |o: ObjId| exact.iter().any(|g| g == &vec![o]);
    partition(cat, Mode::Structure)
        .into_iter()
        .filter(|g| g.len() > 1)
        .map(|g| g.into_iter().filter(|o| singleton(*o)).collect::<Vec<_>>())
        .filter(|g| g.len() > 1)
        .collect()
}

fn local_profile(cat: &Category, o: ObjId, mode: Mode) -> String {
    let x = cat.object(o);
    let about = match mode {
        Mode::Exact => x
            .about
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_default(),
        Mode::Structure => String::new(),
    };
    let frame_question = match mode {
        Mode::Exact => x
            .frame
            .instructions
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_default(),
        Mode::Structure => String::new(),
    };
    format!(
        "{:?}|{:?}|{frame_question}|needs:{:?}|entry:{}|join:{:?}|about:{about}",
        x.frame.primitive,
        x.closure,
        x.entry.needs.iter().collect::<BTreeSet<_>>(),
        x.entry
            .require
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_default(),
        x.join,
    )
}

/// An arrow's label: everything but its name and endpoints.
fn label(cat: &Category, a: ArrowId, mode: Mode) -> String {
    let x = cat.arrow(a);
    let instructions = match mode {
        Mode::Exact => x
            .instructions
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_default(),
        Mode::Structure => String::new(),
    };
    format!(
        "{instructions}|req:{}|att:{}|lvl:{:?}|+{:?}|-{:?}",
        x.require
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_default(),
        x.attested
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_default(),
        x.level,
        x.ensures.iter().collect::<BTreeSet<_>>(),
        x.revokes.iter().collect::<BTreeSet<_>>(),
    )
}

/// Dense class numbers by first occurrence of each distinct key.
fn renumber(keys: &[String]) -> Vec<usize> {
    let mut ids: BTreeMap<&str, usize> = BTreeMap::new();
    keys.iter()
        .map(|k| {
            let next = ids.len();
            *ids.entry(k.as_str()).or_insert(next)
        })
        .collect()
}
