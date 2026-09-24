//! onto-runtime: the async core loop.
//!
//! Many walks run at once. Each step claims its decision frame
//! ([`frames`]), asks a System-1 [`model::Judge`] (Jev, a local judge) and,
//! when the frame runs out, a System-2 [`model::Proposer`] (an OpenRouter
//! model). The model contract is `onto-models`, re-exported as [`model`];
//! remote clients live in `onto-remote`. Calls
//! from different walks run in parallel; walks whose frames could intersect
//! wait for each other, and every intersection is logged as a potentiality.
//! See `docs/00-architecture.md` §5.

pub mod curation;
pub mod discovery;
pub mod engine;
pub mod ensemble;
pub mod frames;
pub mod joins;
pub mod lens;
pub mod mem;
pub mod memory;
pub mod model;
pub mod record;
pub mod supervisor;
pub mod telemetry;
pub mod trace;

pub use engine::{Config, Engine, Job, RunReport};
pub use frames::Policy;
