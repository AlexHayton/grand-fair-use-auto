//! The deterministic game simulation.
//!
//! The simulation is a pure function `(state, tick inputs) -> state`. It has
//! no floats, no I/O, no clock and no platform crates, so a replay of
//! `(tick, input)` pairs gives the same state hashes natively and in wasm.
//! Behaviour is ported from AmiGTA1 (milestones M3 to M7).
#![deny(clippy::float_arithmetic)]
