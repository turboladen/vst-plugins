//! # Plugin Parameters
//!
//! Parameters are the knobs and sliders the user sees in the DAW. Each
//! parameter has:
//!
//! - A **unique string ID** (`#[id = "..."]`) that the host uses to
//!   save and recall presets. Once published, never change these IDs
//!   or existing presets will break.
//! - A **human-readable name** shown in the DAW's UI.
//! - A **range** (min, max, and optional skew).
//! - A **default value**.
//! - Optional **smoothing** to prevent audible clicks when values change.
//!
//! ## Parameter Smoothing
//!
//! When a user moves a knob, the parameter value jumps instantly. But in
//! audio, instant value changes create discontinuities that sound like
//! clicks or "zipper noise." Smoothing gradually ramps the value from
//! old to new over a short time window (e.g., 20ms), eliminating these
//! artifacts. The `SmoothingStyle::Linear(ms)` option ramps linearly
//! over the given duration.

use nih_plug::prelude::*;

/// All user-facing parameters for the Loveless Delay plugin.
///
/// The `#[derive(Params)]` macro automatically generates the code that
/// registers these parameters with the host DAW, handles serialization
/// for presets, and manages parameter smoothing.
#[derive(Params)]
pub struct PluginParams {
    /// **Delay Time** — how long before you hear the echo.
    ///
    /// Controls the distance (in time) between the original signal and
    /// the first repeat. Musically, shorter delays (100-300ms) create
    /// slapback effects, while longer delays (500-2000ms) create
    /// distinct, separated echoes.
    ///
    /// Range: 100ms to 2000ms (2 seconds)
    /// Default: 500ms — a classic delay time that works in most tempos.
    ///
    /// We use a *skewed* range so that roughly half the knob travel covers
    /// 100-500ms (where small changes matter most) and the other half
    /// covers 500-2000ms. This matches how humans perceive time
    /// differences: 100→200ms feels like a big change, but 1800→1900ms
    /// barely registers.
    #[id = "delay"]
    pub delay_time: FloatParam,

    /// **Feedback** — how many times the echo repeats.
    ///
    /// Controls how much of the delayed output is fed back into the delay
    /// input. This creates the recursive loop that produces multiple echoes.
    ///
    /// - 0% = one echo only ("slapback")
    /// - 40% = several echoes, fading naturally
    /// - 95% = very long, slowly decaying repeats
    ///
    /// We cap at 95% for safety. At 100%, the signal would never decay
    /// (infinite repeats at the same volume). Above 100%, the signal
    /// would *grow* with each repeat, quickly clipping to distortion.
    /// The 95% cap provides extremely long tails while staying stable.
    #[id = "fdbk"]
    pub feedback: FloatParam,

    /// **Mix** — the balance between dry (original) and wet (delayed) signal.
    ///
    /// - 0% = fully dry (you hear only the original, no delay at all)
    /// - 50% = equal blend (the default; good for most uses)
    /// - 100% = fully wet (you hear only the delayed signal)
    ///
    /// When used as a send effect in a DAW (aux/bus routing), you'd
    /// typically set this to 100% because the DAW handles the dry/wet
    /// balance. When used as an insert effect, 30-50% is typical.
    #[id = "mix"]
    pub mix: FloatParam,

    /// **Filter Cutoff** — controls how dark the echoes become over time.
    ///
    /// This sets the cutoff frequency of a lowpass filter applied to the
    /// feedback path. Lower values = darker/warmer repeats. Higher values
    /// = brighter/cleaner repeats.
    ///
    /// - 200 Hz = very dark, muffled repeats (like a tape echo)
    /// - 2000 Hz = warm repeats with some presence
    /// - 8000 Hz = natural-sounding with gentle top-end rolloff
    /// - 20000 Hz = essentially no filtering (all frequencies pass)
    ///
    /// The skewed range gives more knob resolution to lower frequencies,
    /// where the sonic differences are more dramatic.
    #[id = "filt"]
    pub filter_cutoff: FloatParam,
}

impl Default for PluginParams {
    fn default() -> Self {
        Self {
            delay_time: FloatParam::new(
                "Delay Time",
                500.0, // Default: 500ms
                FloatRange::Skewed {
                    min: 100.0,
                    max: 2000.0,
                    // `skew_factor(-1.0)` biases the knob toward lower values.
                    // Negative = more resolution at the low end.
                    // Positive = more resolution at the high end.
                    // 0.0 = linear (no skew).
                    factor: FloatRange::skew_factor(-1.0),
                },
            )
            .with_unit(" ms")
            // Smooth delay time changes over 50ms to avoid clicks when
            // the read position in the ring buffer jumps.
            .with_smoother(SmoothingStyle::Linear(50.0))
            // Snap to 0.1ms increments in the DAW UI. Sub-millisecond
            // precision isn't perceptually meaningful for delay time.
            .with_step_size(0.1),

            feedback: FloatParam::new(
                "Feedback",
                0.40, // Default: 40% — a moderate number of repeats
                FloatRange::Linear {
                    min: 0.0,
                    max: 0.95, // Capped below 1.0 for stability
                },
            )
            .with_unit("%")
            .with_smoother(SmoothingStyle::Linear(20.0))
            // Display as percentage: 0.40 → "40.0%"
            .with_value_to_string(formatters::v2s_f32_percentage(1))
            .with_string_to_value(formatters::s2v_f32_percentage()),

            mix: FloatParam::new(
                "Mix",
                0.50, // Default: 50% — equal dry/wet blend
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit("%")
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_value_to_string(formatters::v2s_f32_percentage(1))
            .with_string_to_value(formatters::s2v_f32_percentage()),

            filter_cutoff: FloatParam::new(
                "Filter",
                8000.0, // Default: 8 kHz — gentle high-end rolloff
                FloatRange::Skewed {
                    min: 200.0,
                    max: 20000.0,
                    // Stronger skew (-2.0) for frequency because human
                    // frequency perception is roughly logarithmic.
                    // The difference between 200 Hz and 400 Hz is huge;
                    // the difference between 19800 Hz and 20000 Hz is
                    // imperceptible. This skew matches perception.
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" Hz")
            .with_smoother(SmoothingStyle::Linear(50.0))
            .with_step_size(1.0), // Whole Hz steps are fine
        }
    }
}
