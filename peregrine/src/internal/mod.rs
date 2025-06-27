//! Internal implementation details for the Peregrine simulation engine.
//!
//! This module contains all internal types, traits, and functions that are
//! not part of the public API. Almost all of them need to be exposed anyway
//! so they can be used by generated macro code, but they are hidden in the docs.

pub mod exec;
pub mod history;
pub mod macro_prelude;
pub mod operation;
pub mod placement;
pub mod resource;
pub mod timeline;
