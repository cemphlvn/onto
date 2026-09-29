//! onto-core: a category engine that System-1 decision models walk.
//!
//! See `docs/00-architecture.md` for the design.

pub mod attest;
pub mod category;
pub mod discover;
pub mod ensemble;
pub mod equality;
pub mod error;
pub mod functor;
pub mod laws;
pub mod parse;
pub mod path;
pub mod quotient;
pub mod require;
pub mod state;
pub mod supervise;
pub mod walk;

pub use category::{
    Admission, Arrow, ArrowId, ArrowMeta, Capability, Category, CategoryBuilder, Closure, Entry,
    Frame, Gate, Invariant, Join, ObjId, Object, PathSpec, Primitive, ProposalShape,
};
pub use equality::{Equality, Verdict};
pub use error::Error;
pub use functor::{Functor, FunctorDecl, Obligation};
pub use parse::{Module, parse, parse_module, parse_module_at};
pub use path::Path;
pub use require::Require;
pub use state::{CaseView, StateSpec};
