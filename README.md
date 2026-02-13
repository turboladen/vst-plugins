# Loveless Delay V1

A delay VST3/CLAP plugin built in Rust with [nih-plug](https://github.com/robbert-vdh/nih-plug), designed as a learning project for DSP fundamentals.

## Features

- **Delay Time** — 100ms to 2000ms with skewed knob response
- **Feedback** — 0% to 95% with stability-safe cap
- **Dry/Wet Mix** — 0% to 100%
- **Lowpass Filter** — 200 Hz to 20 kHz on the feedback path, darkens repeats over time

## Signal Flow

```
Input ──┬──────────────────────────────────────── × (1 - mix) ──┐
        │                                                        │
        │   ┌──────────────────────────────────────────────┐     │
        │   │              FEEDBACK LOOP                    │     │
        └──►(+)──► [Ring Buffer] ──► [Lowpass] ──► × feedback ──┘
                                         │                       │
                                    delayed sample               │
                                         └──── × mix ──────────►(+)──► Output
```

## Building

### Prerequisites

- [Rust](https://rustup.rs/) (stable toolchain)
- macOS (for VST3/CLAP bundle signing)

### Build the plugin bundle

```bash
cargo run --manifest-path xtask/Cargo.toml -- bundle loveless-delay-v1 --release
```

This produces both formats in `target/bundled/`:

- `loveless-delay-v1.vst3` — for Logic Pro, Ableton, Cubase, etc.
- `loveless-delay-v1.clap` — for Bitwig, Reaper, and other CLAP hosts

### Run tests

```bash
cargo test
```

### Lint and format

```bash
cargo clippy
cargo fmt --check
```

## Installing in Logic Pro

1. Copy the VST3 bundle to your system plugin folder:

   ```bash
   cp -r "target/bundled/loveless-delay-v1.vst3" ~/Library/Audio/Plug-Ins/VST3/
   ```

2. Open **Logic Pro → Settings → Plug-in Manager**.

3. Click **Reset & Rescan Selection** if the plugin doesn't appear automatically.

4. Create an audio track, click an empty **Insert** slot, and look for **Loveless Audio → Loveless Delay** in the Delay category.

5. If macOS blocks the plugin, go to **System Settings → Privacy & Security** and click **Allow Anyway**, then rescan in Logic Pro.

### Quick rebuild-and-install workflow

```bash
cargo run --manifest-path xtask/Cargo.toml -- bundle loveless-delay-v1 --release && \
  cp -r "target/bundled/loveless-delay-v1.vst3" ~/Library/Audio/Plug-Ins/VST3/
```

## Project Structure

```
src/
├── lib.rs              Plugin entry point, Plugin trait impl, process() algorithm
├── params.rs           Parameter definitions (delay, feedback, mix, filter cutoff)
└── dsp/
    ├── mod.rs           Module declarations
    ├── delay_line.rs    Ring buffer with linear interpolation
    └── filter.rs        One-pole lowpass filter
xtask/                   Build tooling for VST3/CLAP bundling
```

## Architecture Notes

- **Per-sample processing** for clarity over performance — each DSP step is a single readable line.
- **Custom ring buffer** instead of an external crate, with every line commented for learning.
- **Linear interpolation** for fractional delay times, preventing zipper noise during automation.
- **`assert_process_allocs`** enabled in debug builds to catch accidental heap allocations in the audio thread.
- **Parameter smoothing** on all knobs to prevent clicks during value changes.

## DSP Concepts You Now Know

Building this plugin covers the core ideas behind most time-based audio effects. Here's what each piece teaches and where it leads.

### The Ring Buffer Is Everywhere

The delay line at the heart of this plugin — a circular buffer with a write head and a read head — is the same primitive used in reverbs, chorus, flangers, phasers, and comb filters. The only differences are the delay time range and whether the read position moves:

| Effect | Delay Range | Modulated? | Key Difference |
|--------|-------------|------------|----------------|
| **This delay** | 100–2000 ms | No | Fixed read offset, feedback loop |
| **Slapback** | 40–120 ms | No | Short delay, low/no feedback |
| **Chorus** | 10–30 ms | Yes (LFO) | Slow modulation, subtle pitch shift |
| **Flanger** | 1–10 ms | Yes (LFO) | Fast modulation, comb filtering |
| **Phaser** | All-pass filters | Yes (LFO) | Phase shift instead of time delay |
| **Reverb** | Multiple taps | No | Many delay lines in parallel/series |

### Feedback Creates Recursion

The feedback loop (`output → filter → scale → add back to input`) is a recursive system. The math behind it is a geometric series: with feedback `f`, the Nth repeat has amplitude `f^N`. This is why `f < 1.0` decays to silence (the series converges) and `f >= 1.0` doesn't (the series diverges or sustains forever). Capping at 0.95 means the signal drops to ~1% amplitude after about 88 repeats — long enough to sound like it fades forever, short enough to stay stable.

### The One-Pole Filter Is a Building Block

The `y[n] = (1-a)*x + a*y_prev` equation is the simplest IIR filter, but it's a genuine building block. Stack two of them and you get a two-pole (12 dB/oct) filter. Rearrange the signs and it becomes a highpass filter. Use it to smooth a control signal and you have an envelope follower. The coefficient formula `a = e^(-2π*f/sr)` maps between the intuitive world (Hz) and the math world (coefficients), and the same formula appears in synth envelope generators, parameter smoothing, and anywhere you need exponential decay.

### Linear Interpolation vs. Higher Orders

Our delay line uses linear interpolation between adjacent samples for fractional delay times. This is transparent for delay effects but introduces subtle high-frequency rolloff. Professional chorus and pitch-shifting plugins often use cubic (Hermite) or sinc interpolation, reading 4–16 neighboring samples instead of 2. The tradeoff is always quality vs. CPU cost. For a delay where the read position changes slowly, linear interpolation is inaudible — but if you build a pitch shifter where the read position races through the buffer, you'd want to upgrade.

### Real-Time Audio Constraints

The `assert_process_allocs` feature enforces a rule that might seem extreme: *never allocate memory inside `process()`*. This exists because `malloc` can acquire a lock, and locks can block the audio thread for milliseconds — long enough to cause an audible dropout. This is why we pre-allocate the ring buffer in `initialize()` and why the `process()` function uses only stack variables and pre-existing heap data. The same constraint applies to file I/O, networking, and any system call that might block. Every audio plugin framework has this rule; nih-plug just enforces it at compile time.

## What to Explore Next

These are roughly ordered by complexity. Each one builds on what you've already implemented.

1. **Tempo sync** — Read the host BPM from `ProcessContext` and snap delay time to musical subdivisions (1/4 note, 1/8 note, dotted, triplet). This replaces the millisecond parameter with an enum parameter.

2. **Ping-pong delay** — Add a second read from the buffer with the channels swapped: left input feeds right delay and vice versa. The echoes bounce between speakers. Requires only a small change to the channel processing loop.

3. **LFO modulation on delay time** — Add a sine oscillator that modulates `delay_samples` by a few milliseconds. With short base delay (10–30ms), this becomes a chorus effect. With very short delay (1–5ms), it becomes a flanger. You already have the delay line and interpolation — you just need the oscillator.

4. **Biquad filter upgrade** — Replace the one-pole lowpass with a biquad (second-order) filter for a steeper 12 dB/octave rolloff and the ability to do bandpass, highpass, and notch filtering. This adds resonance control and more dramatic tonal shaping of the feedback.

5. **GUI with egui or VIZIA** — nih-plug has built-in support for both. Start with a simple panel showing four sliders, then add a waveform display or a delay time visualization. The `nih-plug` examples include GUI starter templates.

6. **Saturation on the feedback path** — Add a `tanh()` waveshaper before or after the filter in the feedback loop. This soft-clips the signal on each pass, simulating tape saturation. Stacking gentle saturation across many feedback iterations produces a warm, compressed decay character.

7. **Multi-tap delay** — Read from the buffer at multiple offsets (e.g., 1/4, 1/2, 3/4 of the delay time) and mix the taps together. This creates rhythmic patterns from a single delay line without needing multiple buffers.

## License

GPL-3.0-or-later
