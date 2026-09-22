//! Paths: composable chains of arrows.

use crate::category::{ArrowId, Category, ObjId};
use crate::error::Error;

/// A well-typed path `src → dst`.
///
/// Arrows are stored in application order: `[f, g]` means "first f, then g",
/// written `g.f` (that is, `g ∘ f`). An empty path is the identity on `src`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Path {
    pub src: ObjId,
    pub dst: ObjId,
    pub arrows: Vec<ArrowId>,
}

impl Path {
    pub fn id(obj: ObjId) -> Self {
        Self {
            src: obj,
            dst: obj,
            arrows: Vec::new(),
        }
    }

    pub fn is_id(&self) -> bool {
        self.arrows.is_empty()
    }

    /// Extends the path by one arrow (post-composition).
    pub fn push(&mut self, cat: &Category, arrow: ArrowId) -> Result<(), Error> {
        let a = cat.arrow(arrow);
        if a.src != self.dst {
            return Err(Error::NotComposable {
                path: self.display(cat),
                arrow: a.name.clone(),
                expected: cat.object(self.dst).name.clone(),
                found: cat.object(a.src).name.clone(),
            });
        }
        self.arrows.push(arrow);
        self.dst = a.dst;
        Ok(())
    }

    /// Mathematical notation: `g.f` for "f then g", `id(A)` for identities.
    pub fn display(&self, cat: &Category) -> String {
        if self.is_id() {
            return format!("id({})", cat.object(self.src).name);
        }
        self.arrows
            .iter()
            .rev()
            .map(|a| cat.arrow(*a).name.as_str())
            .collect::<Vec<_>>()
            .join(".")
    }

    /// `display` plus its type, e.g. `g.f : A -> C`.
    pub fn display_typed(&self, cat: &Category) -> String {
        format!(
            "{} : {} -> {}",
            self.display(cat),
            cat.object(self.src).name,
            cat.object(self.dst).name
        )
    }
}
