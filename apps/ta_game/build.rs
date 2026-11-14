//! The executable's icon and version information (Windows), and the commit it is built
//! from.

include!("../../branding/windows_resources.rs");
include!("../build_info.rs");

fn main() {
    embed_windows_resources("ta-reborn", "TA Reborn");
    emit_build_info();
    // A main thread with room to spare. A graphics driver does its work, compiling shaders as
    // well, on the thread that calls it, here the main one, which Windows gives 1 MiB: NVIDIA's
    // Vulkan driver (nvoglv64.dll) ran out of it as a player's RTX 4090 drew a battle ("thread
    // 'main' has overflowed its stack"). Linux and macOS give it 8 MiB. (Address space only:
    // memory is taken as the stack is used.)
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        match std::env::var("CARGO_CFG_TARGET_ENV").as_deref() {
            Ok("msvc") => println!("cargo:rustc-link-arg-bins=/STACK:16777216"),
            Ok("gnu") => println!("cargo:rustc-link-arg-bins=-Wl,--stack,16777216"),
            _ => {}
        }
    }
}
