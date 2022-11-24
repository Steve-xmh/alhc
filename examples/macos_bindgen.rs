#[cfg(not(target_os = "macos"))]
fn main() {}

#[cfg(target_os = "macos")]
fn main() {
    let output = std::process::Command::new("xcrun")
        .arg("--sdk")
        .arg("macosx")
        .arg("--show-sdk-path")
        .output()
        .unwrap();
    let output = String::from_utf8_lossy(&output.stdout).to_string();
    let sysroot = output.trim();

    let bindgen_prefix = r#"
#![allow(unused)]
#![allow(non_snake_case)]

use core_foundation::runloop::*;
use core_foundation::data::*;
use core_foundation::dictionary::*;
use core_foundation::array::*;
use core_foundation::string::*;
use core_foundation::url::*;

#[repr(C)]
pub struct __CFAllocator(::core::ffi::c_void);
#[repr(C)]
pub struct __CFError(::core::ffi::c_void);
#[repr(C)]
pub struct __CFReadStream(::core::ffi::c_void);
#[repr(C)]
pub struct __CFWriteStream(::core::ffi::c_void);
#[repr(C)]
pub struct __CFHost(::core::ffi::c_void);
#[repr(C)]
pub struct __CFNetService(::core::ffi::c_void);
#[repr(C)]
pub struct __CFNetServiceMonitor(::core::ffi::c_void);
#[repr(C)]
pub struct __CFNetServiceBrowser(::core::ffi::c_void);
#[repr(C)]
pub struct __CFHTTPMessage(::core::ffi::c_void);
#[repr(C)]
pub struct _CFHTTPAuthentication(::core::ffi::c_void);
#[repr(C)]
pub struct __CFNetDiagnostic(::core::ffi::c_void);

"#;

    let r = bindgen::Builder::default()
        .header("./src/macos/wrapper.h")
        .merge_extern_blocks(true)
        .use_core()
        .generate_comments(true)
        .allowlist_file(".*CFNetwork.framework/Headers/.*")
        .blocklist_item("_+.*")
        .generate_block(true)
        .sort_semantically(true)
        .clang_arg(format!("-isysroot{}", sysroot))
        .generate()
        .unwrap();

    let r = r.to_string();

    let r = r.replace(
        "extern \"C\" {",
        r#"
#[link(name = "CFNetwork", kind = "framework")]
extern "C" {"#,
    );

    std::fs::write(
        "./src/macos/cf_network.rs",
        format!("{}{}", bindgen_prefix, r),
    )
    .unwrap();
}
