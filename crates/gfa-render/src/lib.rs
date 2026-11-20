//! The wgpu renderer.
//!
//! Milestone M0 provides only device setup and a clear pass. The city mesh,
//! sprites, palettes and HUD arrive in M2.
//!
//! Two rules hold from the start:
//! - **WebGL2 limits everywhere.** The device is created with
//!   [`wgpu::Limits::downlevel_webgl2_defaults`] on every platform, so
//!   anything that works on Metal also works in the browser. Only the
//!   maximum texture size follows the adapter, so a Retina window can have
//!   a full-size surface. Game textures keep to [`ASSET_TEXTURE_MAX`].
//! - **Exact palette colours.** A non-sRGB surface format is preferred, so a
//!   palette byte written by the shader reaches the screen unchanged.

mod gpu;

pub use gpu::{FrameOutcome, Gpu, GpuError, WindowSurface, clear};
pub use wgpu;

/// Largest texture the engine creates for game art: the WebGL2 minimum.
pub const ASSET_TEXTURE_MAX: u32 = 2048;
