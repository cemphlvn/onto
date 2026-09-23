//! onto-core: a category engine that System-1 decision models walk.
//!
//! See `docs/00-architecture.md` for the design.

pub mod attest;
pub mod category;
pub mod equality;
pub mod error;
pub mod laws;
pub mod parse;
pub mod path;
pub mod quotient;
pub mod require;
pub mod supervise;
pub mod walk;

pub use category::{
    Arrow, ArrowId, ArrowMeta, Capability, Category, CategoryBuilder, Closure, Entry, Frame, Gate,
    Invariant, Join, ObjId, Object, PathSpec, Primitive,
};
pub use equality::{Equality, Verdict};
pub use error::Error;
pub use parse::parse;
pub use path::Path;
pub use require::Require;
