# Loveless Delay V1 — development commands
# Run `just --list` to see all available recipes.

# Default recipe: build, test, and lint
default: check test lint

# ─────────────────────────────────────────────────
# Building
# ─────────────────────────────────────────────────

# Build the VST3 and CLAP bundles (release mode)
bundle:
    cargo run --manifest-path xtask/Cargo.toml -- bundle loveless-delay-v1 --release

# Build debug (faster compile, slower plugin, with assert_process_allocs active)
bundle-debug:
    cargo run --manifest-path xtask/Cargo.toml -- bundle loveless-delay-v1

# Build the AU component bundle for Logic Pro (release mode)
bundle-au: bundle
    mkdir -p "target/bundled/loveless-delay-v1.component/Contents/MacOS"
    cp "target/bundled/loveless-delay-v1.clap/Contents/MacOS/loveless-delay-v1" \
       "target/bundled/loveless-delay-v1.component/Contents/MacOS/loveless-delay-v1"
    cp "Info.auv2.plist" \
       "target/bundled/loveless-delay-v1.component/Contents/Info.plist"
    echo "BNDL????" > "target/bundled/loveless-delay-v1.component/Contents/PkgInfo"
    codesign --force --timestamp --deep -s - \
       "target/bundled/loveless-delay-v1.component"
    @echo "Built target/bundled/loveless-delay-v1.component"

# Type-check without producing a binary
check:
    cargo check

# ─────────────────────────────────────────────────
# Installing
# ─────────────────────────────────────────────────

# AU install path (Logic Pro scans this directory)
au_dir := "~/Library/Audio/Plug-Ins/Components"

# VST3 install path
vst3_dir := "~/Library/Audio/Plug-Ins/VST3"

# CLAP install path
clap_dir := "~/Library/Audio/Plug-Ins/CLAP"

# Build and install the AU component to the system plugin folder (Logic Pro)
install: bundle-au
    mkdir -p {{ au_dir }}
    cp -r "target/bundled/loveless-delay-v1.component" {{ au_dir }}/
    xattr -cr {{ au_dir }}/loveless-delay-v1.component
    @echo "Installed to {{ au_dir }}/loveless-delay-v1.component"

# Build and install all formats (AU + VST3 + CLAP)
install-all: bundle-au
    mkdir -p {{ au_dir }}
    mkdir -p {{ vst3_dir }}
    mkdir -p {{ clap_dir }}
    cp -r "target/bundled/loveless-delay-v1.component" {{ au_dir }}/
    cp -r "target/bundled/loveless-delay-v1.vst3" {{ vst3_dir }}/
    cp -r "target/bundled/loveless-delay-v1.clap" {{ clap_dir }}/
    xattr -cr {{ au_dir }}/loveless-delay-v1.component
    xattr -cr {{ vst3_dir }}/loveless-delay-v1.vst3
    xattr -cr {{ clap_dir }}/loveless-delay-v1.clap
    @echo "Installed AU to {{ au_dir }}/"
    @echo "Installed VST3 to {{ vst3_dir }}/"
    @echo "Installed CLAP to {{ clap_dir }}/"

# Uninstall the plugin from all system plugin folders
uninstall:
    rm -rf {{ au_dir }}/loveless-delay-v1.component
    rm -rf {{ vst3_dir }}/loveless-delay-v1.vst3
    rm -rf {{ clap_dir }}/loveless-delay-v1.clap
    @echo "Uninstalled from {{ au_dir }}/, {{ vst3_dir }}/, and {{ clap_dir }}/"

# Validate the AU component with Apple's auval tool
validate: install
    killall -9 AudioComponentRegistrar || true
    auval -a
    auval -strict -v aufx Ldly Lvls

# ─────────────────────────────────────────────────
# Testing & linting
# ─────────────────────────────────────────────────

# Run all unit tests
test:
    cargo test

# Run clippy and check formatting (Rust + Markdown)
lint:
    cargo clippy
    cargo fmt --check
    dprint check

# Auto-format all source files (Rust + Markdown)
fmt:
    cargo fmt
    dprint fmt

# ─────────────────────────────────────────────────
# Convenience
# ─────────────────────────────────────────────────

# Full cycle: format, lint, test, build, install
dev: fmt lint test install

# Clean all build artifacts
clean:
    cargo clean
    rm -rf xtask/target
