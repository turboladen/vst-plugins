//! # DSP (Digital Signal Processing) Primitives
//!
//! This module contains the core building blocks for our delay effect:
//!
//! - **`delay_line`**: A ring buffer that stores past audio samples and
//!   retrieves them after a specified delay. This is the heart of any
//!   time-based audio effect.
//!
//! - **`filter`**: A one-pole lowpass filter that removes high-frequency
//!   content from the feedback signal, simulating the natural darkening
//!   of repeats heard in analog delay units.

pub mod delay_line;
pub mod filter;
