//! The deterministic game simulation.
//!
//! The simulation is a pure function `(state, tick inputs) -> state`. It has
//! no floats, no I/O, no clock and no platform crates, so a replay of
//! `(tick, input)` pairs gives the same state hashes natively and in wasm.
//! Behaviour is ported from AmiGTA1 (MIT), in its units: 16.16 world pixels
//! at 32 to a block, 256-step angles, a 50 Hz tick.
//!
//! - [`world`]: the state and its tick.
//! - [`map`]: who may stand where, ramps, fences, ground height.
//! - [`player`]: the player on foot.
//! - [`vehicle`]: the driven car's physics; [`cars`]: parked cars, getting
//!   in and out, ramps, jumps and walls.
//! - [`trains`]: the elevated lines and trams, built from the rail.
//! - [`traffic`]: the fleet of driven cars; [`route`]: their routes.
//! - [`peds`]: the people on foot.
//! - [`clock`]: the tick and the subsystems' slower clocks.
//! - [`view`]: the fixed area that counts as on screen.
//! - [`input`]: buttons per tick, and recordings of them.
//! - [`script`], [`replay`]: test scripts, run logs and state hashes.
//! - [`script_world`]: what the mission VM may ask of the world.
#![deny(clippy::float_arithmetic)]

pub mod cars;
pub mod clock;
pub mod hash;
pub mod input;
pub mod map;
pub mod peds;
pub mod player;
pub mod replay;
pub mod route;
pub mod script;
pub mod script_world;
pub mod traffic;
pub mod trains;
pub mod vehicle;
pub mod view;
pub mod world;

pub use gfa_fixed;
