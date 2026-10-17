//! Deterministic Sierra-style adventure engine.
//!
//! The core has no clock, no filesystem, and no browser APIs. Hosts feed it a
//! package, input events, and a tick count. Rendering is integer-only into a
//! fixed 384×216 framebuffer so native and WebAssembly builds match.

#![forbid(unsafe_code)]

pub mod cond;
pub mod engine;
pub mod error;
pub mod font;
pub mod geom;
pub mod imageutil;
pub mod model;
pub mod pak;
pub mod parser;
pub mod render;
pub mod rng;
pub mod save;
pub mod walk;

pub use engine::{Engine, HostEvent, Input};
pub use error::{Error, Result};
pub use model::{SCREEN_H, SCREEN_W};
pub use pak::{build_pak, load_pak, ActorPart, PackageParts, RoomPart};
pub use walk::{run_walk, WalkHost};
