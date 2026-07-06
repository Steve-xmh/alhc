/// Build script for ALHC on Unix (Linux + macOS).
///
/// Compiles a tiny C shim that provides non-variadic wrappers
/// for curl_easy_setopt (ARM64 variadic workaround).
///
/// Links dynamically against the system libcurl — no curl
/// development headers are needed at build time. The C shim
/// uses `extern` declarations resolved at link time.
fn main() {
    // Only compile the C shim on Unix targets
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "linux" && target_os != "macos" {
        return;
    }

    // Link dynamically against system libcurl
    // - On macOS: /usr/lib/libcurl.4.dylib (always present)
    // - On Linux: needs libcurl.so (comes with libcurl runtime package)
    println!("cargo:rustc-link-lib=curl");

    // Compile the C shim into a static library
    // No curl headers needed — the C code uses extern declarations.
    cc::Build::new()
        .file("src/unix/curl_shim.c")
        .compile("curl_shim");

    // Rerun build script if the C shim changes
    println!("cargo:rerun-if-changed=src/unix/curl_shim.c");
}
