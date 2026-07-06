//! Aether content plane — canonical packages, patch governance, live fanout.

pub mod api;
pub mod director;
pub mod generation;
pub mod store;

pub use api::{router, AppState, PatchBus, StreamEvent};
pub use store::{
    CandidateStatus, Manifest, PlaneStore, PruneReport, PruneRequest, PublishedPatch,
    RollbackResult,
};
