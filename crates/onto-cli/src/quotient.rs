//! `onto quotient`: behavioural equivalence classes (diagnostics only).

use std::path::PathBuf;

use clap::Args;
use onto_core::quotient::{Mode, label_only_identities, partition, redundant_arrows};

use crate::run::BoxError;

#[derive(Args)]
pub struct QuotientArgs {
    file: PathBuf,
}

pub fn main(args: QuotientArgs) -> Result<(), BoxError> {
    let cat = crate::module::load_category(&args.file)?;
    let name = |o: &onto_core::ObjId| cat.object(*o).name.clone();
    let names = |g: &[onto_core::ObjId]| g.iter().map(name).collect::<Vec<_>>().join(", ");

    let exact = partition(&cat, Mode::Exact);
    let structure = partition(&cat, Mode::Structure);
    println!(
        "category {} · {} objects · {} behavioural classes (exact labels) · {} (structure only)",
        cat.name(),
        cat.objects().len(),
        exact.len(),
        structure.len()
    );
    println!("diagnostics only: nothing is merged; the quotient is never applied to the graph");

    // An object with no description and no outgoing arrows is identified
    // by its name alone (a quiddity, in Bird's sense).
    let bare = |o: &onto_core::ObjId| cat.object(*o).about.is_none() && cat.out(*o).is_empty();
    let (name_only, dups): (Vec<_>, Vec<_>) = exact
        .iter()
        .filter(|g| g.len() > 1)
        .partition(|g| g.iter().all(bare));

    println!("\nduplicates (same descriptions, same behaviour: no observation tells them apart)");
    if dups.is_empty() {
        println!("  none");
    }
    for g in dups {
        println!("  {{{}}}", names(g));
    }

    println!(
        "\nidentity by name only (no description, no behaviour: only the name tells them apart)"
    );
    if name_only.is_empty() {
        println!("  none");
    }
    for g in &name_only {
        println!(
            "  {{{}}}   add an `about` to each, or behaviour that distinguishes them",
            names(g)
        );
    }

    println!("\nidentity by description only (same structure; only their descriptions differ)");
    let label_only = label_only_identities(&cat);
    if label_only.is_empty() {
        println!("  none: every object is told apart by structure (asymmetric)");
    }
    for g in &label_only {
        println!("  {{{}}}", names(g));
    }
    if !label_only.is_empty() {
        println!(
            "  these objects' identities rest on their descriptions (judged by models), not on"
        );
        println!("  their preconditions, effects, contracts or targets (checked by code)");
    }

    println!("\nredundant arrows (same source, label and target as an earlier arrow)");
    let redundant = redundant_arrows(&cat);
    if redundant.is_empty() {
        println!("  none");
    }
    for (earlier, later) in redundant {
        let (e, l) = (cat.arrow(earlier), cat.arrow(later));
        println!(
            "  `{}` repeats `{}` ({} -> {})",
            l.name,
            e.name,
            name(&e.src),
            name(&e.dst)
        );
    }
    Ok(())
}
