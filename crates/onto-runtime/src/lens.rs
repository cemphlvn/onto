//! Functors a walk uses (`docs/05-functors.md` §5): a functor from the
//! walked category, with its target.
//!
//! - **Hierarchy**: a choice frame `grouped by F` is judged in two steps:
//!   first among the images of its arrows (few, coarse), then only within
//!   the chosen fiber.
//! - **Transport**: a functor declared with `transport` turns empty
//!   fibers (arrows the target has at `F(x)` and `x` lacks) into
//!   proposals when a walk escalates at `x`, before any LLM is asked.
//!
//! Learned objects and arrows that came from transport are tracked here,
//! so a transported branch keeps its image and can be extended again.

use std::collections::HashMap;
use std::sync::Arc;

use onto_core::{ArrowId, Category, Functor, ObjId};
use serde::Serialize;

pub struct Lens {
    pub functor: Functor,
    pub target: Arc<Category>,
}

/// The coarse step of a grouped frame, for the frame record.
#[derive(Clone, Debug, Serialize)]
pub struct GroupRecord {
    pub functor: String,
    /// The coarse options: images of the frame's arrows.
    pub groups: Vec<String>,
    /// The group chosen, if the judge was confident.
    pub chose: Option<String>,
    pub p: Option<f32>,
    pub confidence: Option<f32>,
    /// Arrows left to judge (the fiber, or every arrow on fallback).
    pub kept: usize,
    /// Not confident at the coarse level: every arrow was judged.
    pub fallback: bool,
}

/// Images of learned structure that came from transport.
#[derive(Default)]
pub struct Transported {
    /// (lens, learned object name) → target object.
    pub objects: HashMap<(usize, String), ObjId>,
    /// (lens, learned arrow name) → target arrow.
    pub arrows: HashMap<(usize, String), ArrowId>,
}

impl Transported {
    /// The image of `x` under lens `li`: declared, or learned by transport.
    pub fn object(&self, li: usize, lens: &Lens, cat: &Category, x: ObjId) -> Option<ObjId> {
        lens.functor
            .object(x)
            .or_else(|| self.objects.get(&(li, cat.object(x).name.clone())).copied())
    }

    /// The first arrow of `f`'s image under lens `li`.
    pub fn first(&self, li: usize, lens: &Lens, cat: &Category, f: ArrowId) -> Option<ArrowId> {
        match lens.functor.arrows.get(f.0 as usize) {
            Some(Some(p)) => p.arrows.first().copied(),
            _ => self.arrows.get(&(li, cat.arrow(f).name.clone())).copied(),
        }
    }
}
