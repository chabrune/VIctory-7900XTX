//! Shared plumbing for every VIctory crate and tool.
//!
//! * [`config`]: where the user's files live (never inside the repository),
//!   and the PC settings that come with them.
//! * [`display`]: windowed, borderless or fullscreen; the screen, VSync and
//!   the frame cap.
//! * [`input`]: the keys that stand in for a gamepad.
//! * [`policy`]: the runtime half of rule #1, refusing to write
//!   game-derived data into the source tree.
//! * [`logging`]: one tracing setup for all binaries, on the console and in
//!   a file.

pub mod config;
pub mod display;
pub mod input;
pub mod logging;
pub mod policy;

pub use config::Config;
pub use policy::{PolicyError, ensure_outside_repo};
