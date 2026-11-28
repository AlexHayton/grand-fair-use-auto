//! The wgpu renderer.
//!
//! - [`city_mesh`] builds the city's geometry from a map, on the CPU.
//! - [`style_tables`] turns a style's tiles and colours into GPU tables.
//! - [`city`] draws the city with them, palette-indexed, in WGSL.
//! - [`sprites`] draws cars, people and objects as instanced quads.
//! - [`camera`] is the perspective camera and its follow behaviour.
//! - [`offscreen`] renders without a window, for tools and tests.
//!
//! Two rules hold throughout:
//! - **WebGL2 limits everywhere.** The device is created with
//!   [`wgpu::Limits::downlevel_webgl2_defaults`] on every platform, so
//!   anything that works on Metal also works in the browser. Only the
//!   maximum texture size follows the adapter, so a Retina window can have
//!   a full-size surface. Game textures keep to [`ASSET_TEXTURE_MAX`].
//! - **Exact palette colours.** A non-sRGB surface format is preferred, so a
//!   palette byte written by the shader reaches the screen unchanged.

pub mod camera;
pub mod city;
pub mod city_mesh;
mod gpu;
pub mod hud;
#[cfg(not(target_arch = "wasm32"))]
pub mod offscreen;
pub mod sprites;
pub mod style_tables;

pub use gpu::{FrameOutcome, Gpu, GpuError, WindowSurface, clear};
pub use {glam, wgpu};

/// Largest texture the engine creates for game art: the WebGL2 minimum.
pub const ASSET_TEXTURE_MAX: u32 = 2048;
