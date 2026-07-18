# Async Lightweight HTTP Client (aka ALHC)

> **WARNING**
>
> This library is still in development and **VERY UNSTABLE**, please don't use it in production environment.

[<img alt="github.com" src="https://img.shields.io/github/stars/Steve-xmh/alhc.svg?label=Github&logo=github">](https://github.com/Steve-xmh/alhc)
[<img alt="crates.io" src="https://img.shields.io/crates/v/alhc.svg?logo=rust">](https://crates.io/crates/alhc)
[<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-alhc?logo=docs.rs">](https://docs.rs/alhc)

What if we need async but also lightweight http client without using such a large library like `reqwest`, `isahc` or `surf`?

ALHC is a async http client library that using System library to reduce binary size and provide async request feature.

HTTPS Example:

```rust
use alhc::prelude::*;
use alhc::*;

use pollster::FutureExt;

fn main() -> DynResult {
    let client = get_client_builder().build().unwrap();

    let r = client
        .post("https://httpbin.org/anything")?
        .header("user-agent", "alhc/0.2.0")
        .body_string("Hello World!".repeat(20))
        .block_on()?
        .recv_string()
        .block_on()?;

    println!("{r}");

    Ok(())
}
```

Our little request example [`https`](./examples/https.rs) with release build can be 182 KB, which is smaller than `tinyget`'s `http` example. If we use rustc nightly feature plus `build-std` and `panic_immediate_abort`, it'll be incredibly 65 KB!

Currently work in progress and supports Windows through WinHTTP and Unix-like systems through the system libcurl. The Unix backend uses libcurl's Multi API: each client has one lightweight driver thread shared by all of its concurrent requests, without depending on a specific async runtime.

## Platform Status

| Name    | Status  | Note                                                |
| ------- | ------- | --------------------------------------------------- |
| Windows | Working | Async WinHTTP backend                               |
| macOS   | Working | System libcurl Multi backend, one driver per client |
| Linux   | Working | System libcurl Multi backend, one driver per client |

## Features

- `async_t_boxed`: Use `async-trait` instead of `async-t`, which requires 1.75+ version of rustc but with zero-cost. Default is disabled.
- `serde`: Can give you the ability of send/receive json data without manually call `serde_json`. Default is disabled.
- `anyhow`: Use `Result` type from `anyhow` crate instead `Result<T, Box<dyn std::error::Error>>`. Default is disabled.

## Minimum binary size on Unix-like platforms

The Unix backend links directly and dynamically to the system `libcurl`, keeping curl out of the executable for a smaller release binary.

- **macOS:** the system `libcurl` is used automatically.
- **Linux and other Unix-like systems:** make sure the linker can resolve `-lcurl` and that the shared library is available at runtime. On Ubuntu/Debian, install `libcurl4-openssl-dev`.

## Compilation binary size comparison

> Note: the measurements use `cargo +nightly build --target [TARGET] --examples`, `cargo +nightly build --release --target [TARGET] --examples`, and `cargo +nightly build --release -Z build-std=core,alloc,std,panic_abort --target [TARGET] --examples`, together with the release profile in [`Cargo.toml`](./Cargo.toml). Because `panic = "immediate-abort"` requires a matching standard library, the regular release measurements use `panic = "abort"`; the size-optimized measurements use the configured `immediate-abort` strategy with `build-std`.

| Name                                                | Windows (x86_64) | Windows (i686) | Windows (aarch64) | macOS (x86_64) | macOS (aarch64) | Linux (x86_64) |
| --------------------------------------------------- | ---------------: | -------------: | ----------------: | -------------: | --------------: | -------------: |
| example `https`                                     |          468,992 |        402,944 |           296,960 |        955,776 |         997,928 |     18,051,064 |
| example `https` release                             |          181,248 |        162,816 |           200,192 |        315,160 |         321,680 |        850,704 |
| example `https` release with size optimization      |           75,264 |         66,048 |            59,392 |         62,744 |          88,208 |        465,480 |
| example `parallel`                                  |          571,904 |        486,912 |           393,216 |      1,153,736 |       1,205,304 |     19,612,824 |
| example `parallel` release                          |          190,464 |        170,496 |           229,888 |        323,480 |         321,808 |        862,992 |
| example `parallel` release with size optimization   |           80,896 |         71,680 |            66,560 |         66,952 |         104,816 |        469,576 |
| example `sequential`                                |          472,064 |        405,504 |           302,080 |        959,440 |         997,464 |     18,048,624 |
| example `sequential` release                        |          182,784 |        164,864 |           203,264 |        315,240 |         321,760 |        850,704 |
| example `sequential` release with size optimization |           76,800 |         68,096 |            60,928 |         62,816 |         104,752 |        465,480 |
