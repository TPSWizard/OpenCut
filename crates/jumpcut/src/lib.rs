//! Smooth jump cuts (row 106): where a talking-head take was cut inside
//! (words or a pause taken out), a few in-between frames bridge the jump
//! instead of the picture snapping. [`find_jump_cuts`] finds the cuts,
//! [`plan_bridge`] places the bridge's frames round each, and
//! [`motion_between`] with [`in_between`] makes the frames. The desktop
//! editor and the web editor (through WASM) both use this crate, checked
//! against `fixtures/jumpcut/jumpcut.json` (an independent Python
//! reference).

mod cuts;
mod morph;

pub use cuts::*;
pub use morph::*;
