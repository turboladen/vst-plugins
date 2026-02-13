/// The xtask binary delegates entirely to nih_plug_xtask, which provides
/// the `bundle` subcommand. Usage:
///
///   cargo xtask bundle loveless-delay-v1 --release
///
/// This compiles the plugin as a cdylib and packages it into a .vst3
/// bundle at `target/bundled/Loveless Delay.vst3`.
fn main() -> nih_plug_xtask::Result<()> {
    nih_plug_xtask::main()
}
