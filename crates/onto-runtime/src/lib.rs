//! onto-runtime: the async core loop.
//!
//! Many walks run at once. Each step claims its decision frame
//! ([`frames`]), asks a System-1 [`model::Judge`] (Jev) and, when the frame
//! runs out, a System-2 [`model::Proposer`] (an OpenRouter model). Calls
//! from different walks run in parallel; walks whose frames could intersect
//! wait for each other, and every intersection is logged as a potentiality.
//! See `docs/00-architecture.md` §5.

pub mod curation;
pub mod engine;
pub mod frames;
pub mod joins;
pub mod lens;
pub mod mem;
pub mod memory;
pub mod model;
pub mod providers;
pub mod record;
pub mod supervisor;
pub mod telemetry;
pub mod trace;

pub use engine::{Config, Engine, Job, RunReport};
pub use frames::Policy;
