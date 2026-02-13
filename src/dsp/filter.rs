//! # One-Pole Lowpass Filter
//!
//! A one-pole lowpass filter is the simplest possible IIR (Infinite Impulse
//! Response) filter. It removes high-frequency content from a signal while
//! letting low frequencies pass through. We use it on the delay's feedback
//! path to darken successive repeats, mimicking analog delay character.
//!
//! ## The Filter Equation
//!
//! ```text
//! y[n] = (1 - a) * x[n] + a * y[n-1]
//! ```
//!
//! Where:
//! - `x[n]` is the current input sample
//! - `y[n]` is the current output sample
//! - `y[n-1]` is the *previous* output sample (the filter's "memory")
//! - `a` is the filter coefficient (0.0 to ~1.0)
//!
//! This is simply a weighted average between the new input and the
//! previous output. The coefficient `a` controls the balance:
//!
//! - `a = 0.0` → output = input (no filtering, signal passes unchanged)
//! - `a = 0.5` → equal mix of input and previous output (moderate filtering)
//! - `a → 1.0` → output ≈ previous output (extreme filtering, almost frozen)
//!
//! ## Computing the Coefficient from Frequency
//!
//! We want to think in terms of "cutoff frequency in Hz" (intuitive),
//! not raw coefficients (not intuitive). The conversion formula is:
//!
//! ```text
//! a = e^(-2π * cutoff_hz / sample_rate)
//! ```
//!
//! This comes from the bilinear transform of an analog RC lowpass circuit.
//! The exponential maps the desired cutoff frequency to the discrete-time
//! coefficient that produces the same frequency response.
//!
//! ## Why This Filter for Delay Feedback?
//!
//! In analog delay units (tape echoes, bucket-brigade devices), each pass
//! through the circuit naturally loses high-frequency content due to
//! component limitations. This gives the repeats a progressively darker,
//! warmer tone that sounds natural and musical. Our digital one-pole filter
//! approximates this behavior with just one multiply and one add per sample.

use std::f32::consts::PI;

/// A one-pole (6 dB/octave) lowpass filter.
///
/// "One-pole" means the filter's transfer function has a single pole in
/// the z-plane. In practical terms, this means it rolls off high
/// frequencies at 6 dB per octave — a gentle slope that sounds natural
/// for feedback darkening. (A "two-pole" filter, like a biquad, rolls
/// off at 12 dB/octave for a steeper cut.)
pub struct OnePoleFilter {
    /// The filter coefficient, computed from the cutoff frequency.
    /// Higher values = more filtering (lower cutoff).
    /// Range: 0.0 (no filtering) to ~0.999 (extreme filtering).
    coefficient: f32,

    /// The previous output sample — the filter's only state variable.
    /// This is what makes it an "IIR" filter: the output depends on
    /// previous *outputs*, not just previous inputs. An "FIR" filter
    /// only looks at previous inputs.
    prev_output: f32,
}

impl OnePoleFilter {
    /// Create a new filter initialized to passthrough (no filtering).
    ///
    /// With `coefficient = 0.0`, the filter equation becomes:
    /// `y[n] = 1.0 * x[n] + 0.0 * y[n-1] = x[n]`
    /// ...which is just the input, unchanged.
    pub fn new() -> Self {
        Self {
            coefficient: 0.0,
            prev_output: 0.0,
        }
    }

    /// Update the filter coefficient for a given cutoff frequency.
    ///
    /// # Arguments
    /// * `cutoff_hz` - Desired cutoff frequency in Hertz. Frequencies above
    ///   this will be progressively attenuated.
    /// * `sample_rate` - Current audio sample rate (e.g., 44100.0 Hz).
    ///
    /// # The Math
    ///
    /// ```text
    /// coefficient = e^(-2π * cutoff / sample_rate)
    /// ```
    ///
    /// Intuition: at a given sample rate, higher cutoff → smaller exponent
    /// → coefficient closer to 0 → less filtering. Lower cutoff → larger
    /// exponent → coefficient closer to 1 → more filtering.
    ///
    /// Example at 44100 Hz:
    /// - cutoff = 20000 Hz → coeff ≈ 0.07 (barely filtering)
    /// - cutoff = 1000 Hz  → coeff ≈ 0.87 (noticeable filtering)
    /// - cutoff = 100 Hz   → coeff ≈ 0.99 (heavy filtering)
    pub fn set_cutoff(&mut self, cutoff_hz: f32, sample_rate: f32) {
        // Clamp cutoff to a safe range:
        // - Min 20 Hz: prevents coefficient from reaching ~1.0, which
        //   could cause numerical stagnation (the filter "gets stuck")
        // - Max 49% of sample rate: approaching the Nyquist frequency
        //   (sample_rate / 2) makes the math unstable. We stay below it.
        let safe_cutoff = cutoff_hz.clamp(20.0, sample_rate * 0.49);

        self.coefficient = (-2.0 * PI * safe_cutoff / sample_rate).exp();
    }

    /// Process one sample through the filter.
    ///
    /// # The Algorithm
    ///
    /// ```text
    /// output = (1 - a) * input + a * prev_output
    /// ```
    ///
    /// This is a weighted average. When `a` is high (e.g., 0.95), the
    /// output is mostly the previous output with just a tiny bit of the
    /// new input mixed in → heavy smoothing → low cutoff frequency.
    ///
    /// When `a` is low (e.g., 0.05), the output is mostly the new input
    /// → minimal smoothing → high cutoff frequency.
    pub fn process(&mut self, input: f32) -> f32 {
        let output = (1.0 - self.coefficient) * input + self.coefficient * self.prev_output;
        self.prev_output = output;
        output
    }

    /// Reset the filter state to zero.
    ///
    /// Called when playback stops to prevent the filter's "memory" from
    /// leaking into the next playback session. Without this, the first
    /// few samples of a new play might sound wrong because `prev_output`
    /// would still hold a value from the end of the last play.
    pub fn reset(&mut self) {
        self.prev_output = 0.0;
    }
}

// ─────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// With coefficient = 0 (default), the filter should pass input through
    /// unchanged. This is important because it means the plugin sounds
    /// transparent when the filter cutoff is at maximum.
    #[test]
    fn test_passthrough_when_coefficient_zero() {
        let mut filter = OnePoleFilter::new();

        assert!(
            (filter.process(1.0) - 1.0).abs() < 1e-6,
            "Filter should pass 1.0 through unchanged"
        );
        assert!(
            (filter.process(0.5) - 0.5).abs() < 1e-6,
            "Filter should pass 0.5 through unchanged"
        );
        assert!(
            (filter.process(-0.3) - (-0.3)).abs() < 1e-6,
            "Filter should pass -0.3 through unchanged"
        );
    }

    /// A very low cutoff should heavily attenuate a high-frequency signal.
    /// The highest possible frequency in digital audio is the Nyquist
    /// frequency (sample_rate / 2), which we approximate by alternating
    /// between +1 and -1 every sample.
    #[test]
    fn test_filter_attenuates_high_freq() {
        let mut filter = OnePoleFilter::new();
        filter.set_cutoff(100.0, 44100.0); // Very low cutoff

        // Feed in a signal that alternates +1, -1 every sample.
        // This is the highest frequency representable at this sample rate.
        let mut max_output = 0.0_f32;
        for i in 0..1000 {
            let input = if i % 2 == 0 { 1.0 } else { -1.0 };
            let output = filter.process(input);
            max_output = max_output.max(output.abs());
        }

        // A 100 Hz lowpass should reduce a ~22050 Hz signal to almost nothing.
        assert!(
            max_output < 0.05,
            "Expected heavy attenuation, got max output {max_output}"
        );
    }

    /// Verify that the coefficient is in the expected range for different
    /// cutoff frequencies. This catches math errors in set_cutoff().
    #[test]
    fn test_coefficient_range() {
        let mut filter = OnePoleFilter::new();

        // High cutoff (near Nyquist): coefficient should be small (near 0)
        // because the filter is barely doing anything.
        filter.set_cutoff(20000.0, 44100.0);
        assert!(
            filter.coefficient < 0.1,
            "High cutoff should give small coefficient, got {}",
            filter.coefficient
        );

        // Low cutoff: coefficient should be large (near 1)
        // because the filter is aggressively smoothing.
        filter.set_cutoff(20.0, 44100.0);
        assert!(
            filter.coefficient > 0.99,
            "Low cutoff should give large coefficient, got {}",
            filter.coefficient
        );
    }

    /// Verify that reset() clears the filter's memory.
    #[test]
    fn test_reset_clears_state() {
        let mut filter = OnePoleFilter::new();
        filter.set_cutoff(1000.0, 44100.0);

        // Process a sample so prev_output is non-zero.
        filter.process(1.0);
        assert!(
            filter.prev_output.abs() > 0.0,
            "prev_output should be non-zero after processing"
        );

        filter.reset();
        assert!(
            filter.prev_output.abs() < 1e-6,
            "prev_output should be zero after reset"
        );
    }

    /// A DC signal (constant value) should pass through the lowpass
    /// filter unchanged, regardless of cutoff frequency. Lowpass filters
    /// only attenuate frequencies *above* the cutoff; DC (0 Hz) is the
    /// lowest possible frequency and should always pass.
    #[test]
    fn test_dc_passes_through() {
        let mut filter = OnePoleFilter::new();
        filter.set_cutoff(100.0, 44100.0); // Very low cutoff

        // Feed constant 1.0 for many samples. The output should converge to 1.0.
        let mut output = 0.0;
        for _ in 0..10000 {
            output = filter.process(1.0);
        }

        assert!(
            (output - 1.0).abs() < 1e-4,
            "DC signal should pass through lowpass, got {output}"
        );
    }
}
