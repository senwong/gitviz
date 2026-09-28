//! Library root for `gitviz`.
//!
//! Exposing the modules as a library (in addition to the `gitviz` binary)
//! lets integration tests exercise the git/config/layout logic directly.

pub mod config;
pub mod discovery;
pub mod emoji;
pub mod git;
pub mod layout;
pub mod markdown;
pub mod review;
pub mod theme;
pub mod view;
pub mod workspace;
