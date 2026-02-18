# CLAUDE.md

## Prerequisites

- [Rust](https://rustup.rs/) (stable toolchain)
- [just](https://github.com/casey/just) — command runner (`cargo install just`)
- [dprint](https://dprint.dev/) — markdown formatter (`cargo install dprint`)

## Commands

```bash
just dev          # Full cycle: format → lint → test → install
just bundle       # Build VST3 + CLAP bundles (release)
just bundle-au    # Build AU component for Logic Pro (release)
just bundle-debug # Debug build (faster, with assert_process_allocs)
just install      # Build + install AU to ~/Library/Audio/Plug-Ins/Components/
just install-all  # Install all formats (AU + VST3 + CLAP)
just validate     # Install + run Apple's auval validation
just test         # cargo test
just lint         # cargo clippy + cargo fmt --check + dprint check
just fmt          # cargo fmt + dprint fmt
just clean        # Remove all build artifacts (including xtask/target)
just check        # Type-check without producing binary (cargo check)
just uninstall    # Remove all plugin bundles from system plugin folders
```

The xtask bundler is NOT a workspace member — build bundles with `just bundle`, not `cargo build`.

## Architecture

```
src/
├── lib.rs              Plugin entry point: LovelessDelay struct, Plugin trait, process()
├── params.rs           PluginParams with #[derive(Params)], 4 FloatParams
└── dsp/
    ├── mod.rs           Re-exports
    ├── delay_line.rs    Ring buffer with linear interpolation (DelayLine)
    └── filter.rs        One-pole lowpass filter (OnePoleFilter)
xtask/                   nih_plug_xtask bundler (separate crate, not a workspace member)
Info.auv2.plist          Audio Unit component metadata (manufacturer, subtype, type)
```

- `LovelessDelay` owns `Vec<DelayLine>` + `Vec<OnePoleFilter>` (one per channel) and
  `Arc<PluginParams>`
- Buffers allocated in `initialize()`, never in `process()`
- Per-sample processing pattern: `buffer.iter_samples()` → per-channel loop

## Parameters

| Param         | ID        | Range                 | Internal type |
| ------------- | --------- | --------------------- | ------------- |
| Delay Time    | `"delay"` | 100–2000 ms (skewed)  | `FloatParam`  |
| Feedback      | `"fdbk"`  | 0.0–0.95              | `FloatParam`  |
| Mix           | `"mix"`   | 0.0–1.0               | `FloatParam`  |
| Filter Cutoff | `"filt"`  | 200–20000 Hz (skewed) | `FloatParam`  |

## Gotchas

- **Parameter IDs are permanent.** `#[id = "delay"]` is baked into saved presets. Never rename them.
- **No heap allocations in `process()`.** The `assert_process_allocs` feature panics in debug if you
  use `String`, `format!()`, `Vec::push()`, `println!()`, or anything that calls `malloc` inside the
  audio processing loop. All buffers must be pre-allocated in `initialize()`.
- **VST3 class ID must be globally unique.** `*b"LvlssDelay__v001"` in `lib.rs` — change this if
  forking.
- **crate-type is `cdylib`**, not the default `rlib`. This produces a `.dylib` the DAW loads.
- **Feedback capped at 0.95** for stability. Values ≥ 1.0 cause infinite or growing signal.
- **`cargo build` does NOT produce a usable plugin.** You must use `just bundle` (which runs xtask)
  to create the `.vst3`/`.clap` bundles with correct macOS directory structure and code signing.
- **Logic Pro only supports Audio Units.** Not VST3, not CLAP. The AU component is built by
  `just bundle-au`, which repackages the CLAP binary via clap-wrapper-rs into a `.component` bundle.
- **AU component metadata** lives in `Info.auv2.plist`. The `manufacturer` (`Lvls`), `subtype`
  (`Ldly`), and `type` (`aufx`) codes are 4-character identifiers that Logic Pro uses to identify
  the plugin. Changing them will make it appear as a different plugin.
- **macOS Gatekeeper** silently blocks unsigned plugins. `just install` runs `xattr -cr`
  automatically to strip quarantine attributes. If installing manually, always run `xattr -cr` on
  the bundle after copying it.

## Code style

- Educational comments explaining DSP math — preserve this style when adding features
- Custom DSP primitives (no external DSP crates) — everything in `src/dsp/` is from scratch
- Unit tests colocated in each DSP module (`#[cfg(test)] mod tests`)
- Per-sample processing chosen for clarity over block-based performance

## Formatting

- **Rust**: `cargo fmt` (rustfmt)
- **Markdown**: `dprint fmt` — config in `dprint.jsonc` (line width 100, text wrap always)
- **Both at once**: `just fmt`
- Always run `just fmt` before committing to catch both Rust and Markdown formatting

## Testing

Tests live inside `src/dsp/delay_line.rs` and `src/dsp/filter.rs` as `#[cfg(test)]` modules. Run
with `just test` or `cargo test`. All DSP primitives should have tests covering edge cases
(wrapping, silence, reset).
