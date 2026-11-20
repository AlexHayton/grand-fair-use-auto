//! Fixed-point maths for the deterministic simulation, in AmiGTA1's units.
//!
//! - Positions are 16.16 at 32 world pixels per block.
//! - Angles are 256 steps per turn, 0 = north, clockwise.
//! - Trig is a 256-entry Q14 cosine table.
//! - Random numbers come from LCGs with fixed seeds.
//!
//! Ported in milestone M3 from AmiGTA1's `gta_trig.c` and the RNGs in
//! `gta_traffic.c`, `gta_peds.c` and `gta_train.c`.
#![no_std]
#![deny(clippy::float_arithmetic)]
