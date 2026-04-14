//! Historical gRPC module. Now holds the proto-to-parsett mapping
//! helpers the DMM page parser consumes; all transport code
//! (`server.rs`, `handler.rs`, `constants.rs`) was removed in Phase 4
//! when Zilean collapsed to a single in-process binary.
//!
//! A later refactor can inline [`mapping`] into [`crate::dmm`] and retire
//! this module entirely — the name is kept for now so the diff stays
//! focussed.

pub mod mapping;
