//! # Loveless Delay — An AU/VST3/CLAP Delay Plugin
//!
//! A simple delay effect plugin built with [nih-plug](https://github.com/robbert-vdh/nih-plug)
//! for learning DSP fundamentals. Outputs Audio Unit (AUv2), VST3, and CLAP
//! formats from a single codebase. Every algorithm is implemented from scratch
//! with thorough comments explaining the "why" behind each line of DSP code.
//!
//! ## Signal Flow
//!
//! ```text
//! Input ──┬──────────────────────────────────────── × (1 - mix) ───┐
//!         │                                                        │
//!         │    ┌──────────────────────────────────────────────┐    │
//!         │    │              FEEDBACK LOOP                   │    │
//!         │    │                                              │    │
//!         └──►(+)──► [Ring Buffer / Delay Line] ──► [Lowpass] │    │
//!              ▲      (stores & retrieves past     (darkens   │    │
//!              │       samples after N ms)          repeats)  │    │
//!              │                    │                  │      │    │
//!              │                    │                  │      │    │
//!              │                    ▼                  ▼      │    │
//!              │              delayed_sample    × feedback ───┘    │
//!              │                    │                              │
//!              └────────────────────│──────────────────────────────┘
//!                                   │                              │
//!                                   └──── × mix ─────────────────►(+)──► Output
//! ```

mod dsp;
mod params;

use std::num::{NonZeroU32, NonZeroUsize};
use std::sync::Arc;

use dsp::{delay_line::DelayLine, filter::OnePoleFilter};
use nih_plug::prelude::*;
use params::PluginParams;

/// The main plugin struct.
///
/// This holds all the audio-rate state that persists between calls to
/// `process()`. The DAW calls `process()` hundreds of times per second,
/// each time passing a small buffer of audio samples (typically 64-1024
/// samples). Our state must survive between these calls.
///
/// ## Why separate state from parameters?
///
/// Parameters (`PluginParams`) are shared with the host via `Arc` and can
/// be read from any thread (the audio thread, the UI thread, the host's
/// automation thread). Plugin state (delay lines, filters) is owned
/// exclusively by the audio thread and only accessed in `process()`.
/// This separation makes the design thread-safe without locks.
struct LovelessDelay {
    /// Shared reference to the plugin parameters. The `Arc` (Atomic
    /// Reference Counted pointer) allows both the plugin and the host
    /// to hold references to the same parameter data without copying.
    params: Arc<PluginParams>,

    /// The current sample rate in Hz (e.g., 44100.0 or 48000.0).
    /// Set during `initialize()` and used to convert delay time from
    /// milliseconds to samples: `delay_samples = delay_ms * sample_rate / 1000`.
    sample_rate: f32,

    /// One delay line (ring buffer) per audio channel.
    ///
    /// For stereo audio, this will contain 2 independent delay lines.
    /// Each channel is processed separately so that stereo imaging is
    /// preserved — if only the left channel has audio, only the left
    /// delay line produces echoes.
    delay_lines: Vec<DelayLine>,

    /// One lowpass filter per audio channel, applied to the feedback
    /// signal before it re-enters the delay line.
    ///
    /// Independent per-channel filters ensure that stereo balance is
    /// maintained even when the filter cutoff changes.
    filters: Vec<OnePoleFilter>,
}

impl Default for LovelessDelay {
    fn default() -> Self {
        Self {
            params: Arc::new(PluginParams::default()),
            // 44100 Hz is a placeholder. The real sample rate is set in
            // initialize() when the host tells us the actual configuration.
            sample_rate: 44100.0,
            // Empty vecs — populated in initialize() when we know the
            // channel count and sample rate.
            delay_lines: Vec::new(),
            filters: Vec::new(),
        }
    }
}

impl Plugin for LovelessDelay {
    const NAME: &'static str = "Loveless Delay";
    const VENDOR: &'static str = "Loveless Audio";
    const URL: &'static str = "";
    const EMAIL: &'static str = "steve.loveless@gmail.com";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    // Supported audio channel layouts. The host will pick the first
    // layout that matches the track configuration.
    //
    // We support stereo (2 in → 2 out) and mono (1 in → 1 out).
    // Most DAW tracks are stereo, so we list it first.
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        // Stereo layout
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            aux_input_ports: &[],
            aux_output_ports: &[],
            names: PortNames::const_default(),
        },
        // Mono fallback
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(1),
            aux_input_ports: &[],
            aux_output_ports: &[],
            names: PortNames::const_default(),
        },
    ];

    // We don't use MIDI, so disable it to keep things simple.
    const MIDI_INPUT: MidiConfig = MidiConfig::None;

    // Process parameter changes at sample-accurate timing. This means
    // when the host sends an automation point at sample 37 of a buffer,
    // the parameter actually changes at sample 37 (not at the start
    // of the buffer). More accurate, but we're already doing per-sample
    // smoothing so this just ensures consistency.
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    /// Called when the plugin is first loaded, or when the audio
    /// configuration changes (e.g., sample rate change, channel count
    /// change). This is where we allocate our delay buffers.
    ///
    /// # Why allocate here instead of in `default()`?
    ///
    /// We need the sample rate to calculate buffer sizes, and we need
    /// the channel count to create the right number of delay lines.
    /// Both are only known when the host calls `initialize()`.
    ///
    /// # Return value
    ///
    /// Return `true` if initialization succeeded. Returning `false`
    /// tells the host the plugin can't work with this configuration
    /// (e.g., unsupported channel count), and the host won't load it.
    fn initialize(
        &mut self,
        audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.sample_rate = buffer_config.sample_rate;

        // Determine the number of audio channels from the layout.
        let num_channels = audio_io_layout
            .main_input_channels
            .map(|c| c.get() as usize)
            .unwrap_or(2);

        // Calculate the maximum buffer size in samples.
        //
        // Our maximum delay time parameter is 2000ms. We add 100ms of
        // headroom (2100ms total) to account for parameter smoothing
        // overshooting slightly during transitions.
        //
        // Formula: time_seconds * sample_rate = samples
        //   2.1 seconds * 44100 Hz = 92610 samples
        //   2.1 seconds * 48000 Hz = 100800 samples
        //
        // Each sample is an f32 (4 bytes), so at 48 kHz this buffer
        // uses about 400 KB per channel — very modest.
        const MAX_DELAY_SECONDS: f32 = 2.1;
        let max_delay_samples = (MAX_DELAY_SECONDS * self.sample_rate) as usize;

        // Create fresh delay lines and filters for each channel.
        // We replace any existing ones to handle sample rate changes.
        // `NonZeroUsize` guarantees the delay line can't be zero-length,
        // which would cause division-by-zero in ring buffer arithmetic.
        let max_delay_len =
            NonZeroUsize::new(max_delay_samples).expect("max delay samples must be > 0");
        self.delay_lines = (0..num_channels)
            .map(|_| DelayLine::new(max_delay_len))
            .collect();

        self.filters = (0..num_channels).map(|_| OnePoleFilter::new()).collect();

        true // Initialization succeeded
    }

    /// Called when playback stops or the plugin is bypassed.
    ///
    /// We clear all delay buffers and filter states so that stale audio
    /// doesn't bleed into the next playback. Without this, pressing
    /// "play" after "stop" might produce a burst of old echoes.
    fn reset(&mut self) {
        for dl in &mut self.delay_lines {
            dl.clear();
        }
        for f in &mut self.filters {
            f.reset();
        }
    }

    /// The core audio processing function — this is where all the DSP
    /// magic happens.
    ///
    /// The host calls this function repeatedly, passing small buffers
    /// of audio samples. A typical buffer might be 256 samples long at
    /// 44100 Hz, meaning this function is called ~172 times per second.
    ///
    /// # Arguments
    ///
    /// * `buffer` - The audio data. Contains interleaved channels that
    ///   we iterate over. We read input samples and write output samples
    ///   back to the same buffer (in-place processing).
    /// * `_aux` - Auxiliary buffers (sidechain inputs, etc.). Unused.
    /// * `_context` - Process context with transport info. Unused.
    ///
    /// # The Delay Algorithm
    ///
    /// For each sample, across all channels:
    ///
    /// 1. **Read** the delayed sample from the ring buffer
    /// 2. **Filter** it through the lowpass (darkens the feedback)
    /// 3. **Scale** by feedback amount (controls decay rate)
    /// 4. **Write** (input + scaled feedback) into the ring buffer
    /// 5. **Mix** dry and wet signals for the output
    /// 6. **Advance** the ring buffer write position
    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Iterate over the buffer one sample at a time, across all channels.
        //
        // `iter_samples()` yields a `ChannelSamples` for each time step.
        // Within each time step, we process all channels. This is the
        // "per-sample, per-channel" pattern — the clearest (though not
        // the fastest) way to implement audio processing.
        for mut channel_samples in buffer.iter_samples() {
            // ─── Read smoothed parameter values for this sample ───
            //
            // `.smoothed.next()` returns the parameter's current value
            // after applying the smoother. If the user just moved a knob
            // from 500ms to 1000ms, the smoother gradually ramps from
            // 500 to 1000 over the smoothing duration (e.g., 50ms),
            // giving us intermediate values like 501, 502, 503... instead
            // of an instant jump.
            let delay_ms = self.params.delay_time.smoothed.next();
            let feedback = self.params.feedback.smoothed.next();
            let mix = self.params.mix.smoothed.next();
            let filter_cutoff = self.params.filter_cutoff.smoothed.next();

            // Convert delay time from milliseconds to samples.
            //
            // This is one of the most fundamental DSP conversions:
            //
            //   delay_samples = delay_ms * sample_rate / 1000
            //
            // At 44100 Hz:
            //   100ms  =  4410 samples
            //   500ms  = 22050 samples
            //   2000ms = 88200 samples
            //
            // The result is often fractional (e.g., 441.3 samples for
            // 10.007ms), which is why our delay line supports fractional
            // reads via linear interpolation.
            let delay_samps = calculate_delay_samples(delay_ms, self.sample_rate);

            // Process each audio channel independently.
            for (channel_idx, sample) in channel_samples.iter_mut().enumerate() {
                // Get this channel's delay line and filter.
                // The `let-else` pattern skips channels we don't have
                // state for (shouldn't happen after initialize()).
                let Some(delay_line) = self.delay_lines.get_mut(channel_idx) else {
                    continue;
                };
                let Some(filter) = self.filters.get_mut(channel_idx) else {
                    continue;
                };

                // Update the filter's cutoff frequency for this sample.
                // We do this per-sample (not per-buffer) because the
                // cutoff parameter might be smoothing toward a new value,
                // and we want the filter to track that smoothly.
                filter.set_cutoff(filter_cutoff, self.sample_rate);

                // ═══════════════════════════════════════════════════════
                // THE DELAY ALGORITHM — 6 steps per sample
                // ═══════════════════════════════════════════════════════

                // Step 1: READ the delayed sample from the ring buffer.
                //
                // We look backward in time by `delay_samples` samples.
                // If the delay is 500ms at 44100 Hz, we're reading the
                // sample that was written 22050 samples ago. Linear
                // interpolation handles fractional positions.
                let delayed_sample = delay_line.read(delay_samps);

                // Step 2: FILTER the delayed sample through the lowpass.
                //
                // This simulates the high-frequency loss that occurs in
                // analog delay circuits. Each time the signal passes
                // through the feedback loop, it goes through this filter
                // again, so the repeats get progressively darker.
                //
                // First repeat: filtered once (slightly darker)
                // Second repeat: filtered twice (noticeably darker)
                // Third repeat: filtered three times (quite dark)
                // ...and so on.
                let filtered = filter.process(delayed_sample);

                // Step 3: SCALE by the feedback amount.
                //
                // This controls how loud each repeat is relative to
                // the one before it. With feedback = 0.5:
                //   1st repeat: 50% of original volume
                //   2nd repeat: 25% (50% of 50%)
                //   3rd repeat: 12.5% (50% of 25%)
                //
                // The signal decays geometrically. Higher feedback =
                // slower decay = more audible repeats.
                let feedback_sample = filtered * feedback;

                // Step 4: WRITE (input + feedback) into the ring buffer.
                //
                // The current input sample enters the delay line, along
                // with the feedback signal from the previous iteration
                // of the loop. This is what creates the recursion:
                // output feeds back into input, producing echoes of echoes.
                let input_sample = *sample;
                delay_line.write(input_sample + feedback_sample);

                // Step 5: MIX dry (original) and wet (delayed) signals.
                //
                // This is a simple linear crossfade:
                //   output = dry * (1 - mix) + wet * mix
                //
                //   mix = 0.0 → output = input (no delay audible)
                //   mix = 0.5 → output = 50% input + 50% delayed
                //   mix = 1.0 → output = delayed only (input silent)
                *sample = input_sample * (1.0 - mix) + delayed_sample * mix;

                // Step 6: ADVANCE the ring buffer's write position.
                //
                // Move the "write head" forward by one sample, ready for
                // the next sample. The delay line handles the wrapping
                // internally (position resets to 0 at the end of the buffer).
                delay_line.advance();
            }
        }

        // Tell the host how long our effect tail is so it keeps calling
        // process() after the input goes silent (e.g., when a region ends
        // or the track is muted). Without this, the delay echoes would be
        // cut off abruptly.
        //
        // The tail length depends on how many repeats it takes for the
        // feedback loop to decay to -60 dB (inaudible). Each repeat is
        // attenuated by the feedback factor, so after N repeats the level
        // is feedback^N. Solving feedback^N = 0.001 (-60 dB):
        //
        //   N = log(0.001) / log(feedback)
        //
        // Multiply N by the delay time in samples to get the tail length.
        let delay_ms = self.params.delay_time.smoothed.next();
        let feedback = self.params.feedback.smoothed.next();
        let delay_samps = calculate_delay_samples(delay_ms, self.sample_rate);

        let tail_samples = if feedback > 0.001 {
            let repeats = -3.0 / feedback.log10(); // log10(0.001) = -3
            (repeats * delay_samps) as u32
        } else {
            // With no feedback, just one delay period for the single echo.
            delay_samps as u32
        };

        ProcessStatus::Tail(tail_samples)
    }
}

const fn calculate_delay_samples(delay_ms: f32, sample_rate: f32) -> f32 {
    delay_ms * sample_rate / 1000.0
}

// ─────────────────────────────────────────────────────────────────────
// Plugin format trait implementations
// ─────────────────────────────────────────────────────────────────────
//
// These traits tell nih-plug how to package the plugin for different
// plugin formats. We support both CLAP and VST3.

impl ClapPlugin for LovelessDelay {
    // A reverse-domain-notation ID, unique to this plugin.
    const CLAP_ID: &'static str = "com.loveless-audio.loveless-delay-v1";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("A delay plugin with feedback filtering, built for learning DSP");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Delay,
    ];
}

impl Vst3Plugin for LovelessDelay {
    // A 16-byte class ID that must be globally unique across all VST3
    // plugins ever made. For a production plugin, use a proper UUID.
    // For our learning project, this ASCII-based ID is sufficient.
    //
    // The `*b"..."` syntax creates a `[u8; 16]` from a 16-character
    // ASCII string literal. Each character becomes one byte.
    const VST3_CLASS_ID: [u8; 16] = *b"LvlssDelay__v001";

    // Tell the host this is a delay effect so it appears in the
    // correct category in the plugin browser.
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Delay];
}

// ─────────────────────────────────────────────────────────────────────
// Export macros
// ─────────────────────────────────────────────────────────────────────
//
// These macros generate the C-compatible entry points that the host
// DAW uses to discover and load the plugin. Without these, the compiled
// .dylib would have no externally visible symbols and the host wouldn't
// know it's a plugin.
//
// nih_export_clap! exports the `clap_entry` symbol for CLAP hosts.
// nih_export_vst3! exports `GetPluginFactory` for VST3 hosts.
// clap_wrapper re-exports the CLAP entry point as AUv2 and VST3 via
// the clap-wrapper crate, so Logic Pro (Audio Units only) can load it.

nih_export_clap!(LovelessDelay);
nih_export_vst3!(LovelessDelay);

// Wrap our CLAP plugin into AUv2 format for Logic Pro.
// This generates a `GetPluginFactoryAUV2` entry point that macOS uses
// to discover the plugin as an Audio Unit component.
clap_wrapper::export_auv2!();
