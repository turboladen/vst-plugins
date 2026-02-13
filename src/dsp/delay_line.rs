//! # Delay Line (Ring Buffer)
//!
//! A delay line stores audio samples and lets you read them back after a
//! specified time delay. This is the fundamental building block of all
//! delay, reverb, chorus, and flanger effects.
//!
//! ## How a Ring Buffer Works
//!
//! Imagine a circular tape loop. A "write head" records incoming audio
//! onto the tape, and a "read head" plays it back from a position further
//! behind on the tape. The distance between the two heads determines the
//! delay time.
//!
//! In code, we use a `Vec<f32>` as our "tape" and an integer index as the
//! write head position. Each time we process one audio sample:
//!
//! 1. Read the delayed sample from `(write_pos - delay_in_samples)`,
//!    wrapping around to the end of the buffer if we go past the start.
//! 2. Write the new sample at `write_pos`.
//! 3. Advance `write_pos` by 1, wrapping back to 0 at the end.
//!
//! The "ring" in "ring buffer" comes from this circular wrapping behavior:
//! the buffer has no beginning or end, just a continuously moving window
//! of stored samples.
//!
//! ## Linear Interpolation
//!
//! When the delay time isn't an exact whole number of samples (e.g., 441.3
//! samples for a 10.007ms delay at 44100 Hz), we need to interpolate
//! between two adjacent stored samples. Without interpolation, the delay
//! time would snap between whole sample positions, causing audible
//! artifacts called "zipper noise."
//!
//! Linear interpolation blends two neighbors:
//!
//! ```text
//! result = sample_a * (1 - frac) + sample_b * frac
//! ```
//!
//! For position 441.3:
//! - `sample_a` is at position 441 (weight 0.7)
//! - `sample_b` is at position 442 (weight 0.3)
//! - `result = sample_a * 0.7 + sample_b * 0.3`

/// A ring buffer that functions as an audio delay line.
///
/// The buffer is pre-allocated to the maximum possible delay length
/// during `initialize()`, so no memory allocation ever happens during
/// audio processing. This is critical for real-time audio: memory
/// allocation can block (waiting for a lock), causing audio dropouts.
pub struct DelayLine {
    /// The circular buffer storing audio samples. All values start at
    /// 0.0 (silence).
    buffer: Vec<f32>,

    /// Current write position — where the next incoming sample will be
    /// stored. Advances by 1 each sample, wrapping to 0 at `buffer_len`.
    write_pos: usize,

    /// Cached buffer length, stored to avoid repeated `.len()` calls
    /// and to make the modular arithmetic clearer in the code.
    buffer_len: usize,
}

impl DelayLine {
    /// Create a new delay line with the given maximum size in samples.
    ///
    /// # Arguments
    /// * `max_length` - Maximum number of samples to store. For a 2-second
    ///   delay at 44100 Hz, this would be 88200.
    ///
    /// # Why pre-allocate?
    /// We allocate the full buffer up front so that changing the delay
    /// time parameter never triggers a memory allocation. The buffer
    /// stays the same size; only the read position changes.
    pub fn new(max_length: usize) -> Self {
        Self {
            buffer: vec![0.0; max_length],
            write_pos: 0,
            buffer_len: max_length,
        }
    }

    /// Write a sample into the delay line at the current write position.
    ///
    /// **Important:** This does NOT advance the write position. Call
    /// [`advance()`](Self::advance) after both `read()` and `write()` are
    /// complete for the current sample. This separation lets us read the
    /// old value before overwriting it.
    pub fn write(&mut self, sample: f32) {
        self.buffer[self.write_pos] = sample;
    }

    /// Read a delayed sample from the buffer using linear interpolation.
    ///
    /// # Arguments
    /// * `delay_samples` - How many samples back to read. Can be fractional
    ///   (e.g., 441.3) for smooth delay time changes.
    ///
    /// # How the index math works
    ///
    /// To read N samples behind the write head in a circular buffer:
    ///
    /// ```text
    /// read_index = (write_pos + buffer_len - N) % buffer_len
    /// ```
    ///
    /// We add `buffer_len` before subtracting to avoid negative numbers
    /// (Rust's `usize` can't be negative). The modulo (`%`) wraps the
    /// result back into the valid buffer range.
    ///
    /// Example: `write_pos = 5`, `N = 10`, `buffer_len = 100`:
    /// ```text
    /// (5 + 100 - 10) % 100 = 95
    /// ```
    /// Position 95 is indeed 10 steps behind position 5 on a ring of 100.
    pub fn read(&self, delay_samples: f32) -> f32 {
        // Clamp to valid range: at least 0 samples, at most the full buffer.
        let delay_clamped = delay_samples.clamp(0.0, (self.buffer_len - 1) as f32);

        // Split into integer and fractional parts.
        //
        // For delay_samples = 441.3:
        //   delay_int  = 441   (which buffer slots to look at)
        //   delay_frac = 0.3   (how much to blend between them)
        let delay_int = delay_clamped as usize;
        let delay_frac = delay_clamped - delay_int as f32;

        // Calculate two adjacent read positions in the ring buffer.
        // index_a is the "earlier" sample (closer in time to now).
        // index_b is one sample further back (older).
        let index_a = (self.write_pos + self.buffer_len - delay_int) % self.buffer_len;
        let index_b = (self.write_pos + self.buffer_len - delay_int - 1) % self.buffer_len;

        let sample_a = self.buffer[index_a];
        let sample_b = self.buffer[index_b];

        // Linear interpolation: blend between the two adjacent samples
        // based on the fractional part of the delay.
        //
        // When delay_frac = 0.0 → result = sample_a (exact position)
        // When delay_frac = 0.5 → result = average of a and b
        // When delay_frac = 1.0 → result = sample_b (next position)
        //
        // This ensures smooth, artifact-free output when the delay time
        // is changed continuously (e.g., by automating the knob).
        sample_a * (1.0 - delay_frac) + sample_b * delay_frac
    }

    /// Advance the write position by one sample.
    ///
    /// Call this once per sample, after both `read()` and `write()` are
    /// done. The modulo wraps the position back to 0 when it reaches
    /// the end of the buffer, creating the circular behavior.
    pub fn advance(&mut self) {
        self.write_pos = (self.write_pos + 1) % self.buffer_len;
    }

    /// Clear the entire buffer to silence and reset the write position.
    ///
    /// Called during plugin `reset()` (when the user stops playback)
    /// to prevent stale audio from bleeding into the next play session.
    pub fn clear(&mut self) {
        self.buffer.fill(0.0);
        self.write_pos = 0;
    }
}

// ─────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify basic write-then-read at an exact sample position.
    #[test]
    fn test_write_and_read_exact() {
        let mut dl = DelayLine::new(100);

        // Write 0.75 at position 0, then advance to position 1.
        dl.write(0.75);
        dl.advance();

        // Reading 1 sample back should give us the 0.75 we just wrote.
        let result = dl.read(1.0);
        assert!((result - 0.75).abs() < 1e-6, "Expected 0.75, got {result}");
    }

    /// Verify linear interpolation between two samples.
    #[test]
    fn test_interpolation() {
        let mut dl = DelayLine::new(100);

        // Write two known values: 0.0 at pos 0, then 1.0 at pos 1.
        dl.write(0.0);
        dl.advance();
        dl.write(1.0);
        dl.advance();

        // Now write_pos is 2. Reading 1.5 samples back means:
        //   index_a = (2 - 1) = pos 1 → value 1.0 (weight 0.5)
        //   index_b = (2 - 2) = pos 0 → value 0.0 (weight 0.5)
        //   result  = 1.0 * 0.5 + 0.0 * 0.5 = 0.5
        let result = dl.read(1.5);
        assert!((result - 0.5).abs() < 1e-6, "Expected 0.5, got {result}");
    }

    /// Verify the buffer wraps correctly past its boundaries.
    #[test]
    fn test_wrapping() {
        let mut dl = DelayLine::new(4);

        // Write values 0 through 5 into a buffer of size 4.
        // The buffer will contain the last 4 values written.
        for i in 0..6 {
            dl.write(i as f32);
            dl.advance();
        }

        // After 6 writes into size-4 buffer:
        //   write_pos = 6 % 4 = 2
        //   Buffer contents: [4.0, 5.0, 2.0, 3.0]
        //                     pos0  pos1  pos2  pos3
        //   (positions 0 and 1 were overwritten by values 4 and 5)
        //
        // Reading 1 sample back from write_pos 2:
        //   index = (2 + 4 - 1) % 4 = 5 % 4 = 1 → buffer[1] = 5.0
        let result = dl.read(1.0);
        assert!((result - 5.0).abs() < 1e-6, "Expected 5.0, got {result}");
    }

    /// Verify that clearing resets everything to silence.
    #[test]
    fn test_clear() {
        let mut dl = DelayLine::new(10);

        dl.write(0.5);
        dl.advance();
        dl.clear();

        // After clearing, reading anywhere should return 0.0.
        let result = dl.read(1.0);
        assert!(
            result.abs() < 1e-6,
            "Expected 0.0 after clear, got {result}"
        );
    }

    /// A buffer initialized to silence should output silence at any delay.
    #[test]
    fn test_silence_in_silence_out() {
        let dl = DelayLine::new(100);

        for delay in [1.0, 10.0, 50.0, 99.0] {
            let result = dl.read(delay);
            assert!(
                result.abs() < 1e-6,
                "Expected silence at delay {delay}, got {result}"
            );
        }
    }

    /// Verify that writing multiple samples and reading them back
    /// produces the correct sequence (FIFO behavior).
    #[test]
    fn test_fifo_sequence() {
        let mut dl = DelayLine::new(10);

        // Write a recognizable sequence: 1, 2, 3, 4, 5
        for i in 1..=5 {
            dl.write(i as f32);
            dl.advance();
        }

        // Read back in order: most recent first.
        // 1 sample back = 5.0 (most recently written)
        // 2 samples back = 4.0
        // 5 samples back = 1.0 (oldest)
        assert!((dl.read(1.0) - 5.0).abs() < 1e-6);
        assert!((dl.read(2.0) - 4.0).abs() < 1e-6);
        assert!((dl.read(3.0) - 3.0).abs() < 1e-6);
        assert!((dl.read(4.0) - 2.0).abs() < 1e-6);
        assert!((dl.read(5.0) - 1.0).abs() < 1e-6);
    }
}
