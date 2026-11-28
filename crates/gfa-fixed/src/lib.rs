//! Fixed-point maths for the deterministic simulation, in AmiGTA1's units.
//!
//! - [`fx`]: positions are 16.16 at 32 world pixels per block.
//! - [`trig`]: angles are 256 steps per turn, 0 = north, clockwise; a
//!   256-entry Q14 cosine table and a vector-to-angle inverse.
//! - [`rng`]: linear congruential generators with fixed seeds.
//! - [`dos_trig`]: GTA8.EXE's own angles (1024 steps, counter-clockwise),
//!   sine and arctangent, for the parts ported from the original itself.
//!
//! Ported from AmiGTA1 (MIT): `gta_trig.c` and the generators in
//! `gta_traffic.c`, `gta_peds.c`, `gta_train.c` and `gta_main.c`.
#![no_std]
#![deny(clippy::float_arithmetic)]

pub mod dos_trig;
pub mod fx;
pub mod rng;
pub mod trig;
