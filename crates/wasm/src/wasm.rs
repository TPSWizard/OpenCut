#[cfg(target_arch = "wasm32")]
mod compositor;
#[cfg(target_arch = "wasm32")]
mod effects;
#[cfg(target_arch = "wasm32")]
mod gpu;
#[cfg(target_arch = "wasm32")]
mod animation;
#[cfg(target_arch = "wasm32")]
mod masks;
#[cfg(target_arch = "wasm32")]
mod perf;
#[cfg(target_arch = "wasm32")]
mod transitions;
#[cfg(target_arch = "wasm32")]
mod motion_tracking;
#[cfg(target_arch = "wasm32")]
mod voice;

#[cfg(target_arch = "wasm32")]
pub use compositor::*;
#[cfg(target_arch = "wasm32")]
pub use effects::*;
#[cfg(target_arch = "wasm32")]
pub use gpu::*;
#[cfg(target_arch = "wasm32")]
pub use animation::*;
#[cfg(target_arch = "wasm32")]
pub use masks::*;
#[cfg(target_arch = "wasm32")]
pub use perf::*;
#[cfg(target_arch = "wasm32")]
pub use transitions::*;
#[cfg(target_arch = "wasm32")]
pub use motion_tracking::*;
#[cfg(target_arch = "wasm32")]
pub use voice::*;
pub use time::*;
