# Async Lightweight HTTP Client (aka ALHC)

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

Our little request example [`https`](./examples/https.rs) with release build can be 182 KB, which is smaller than `tinyget`'s `http` example. If we use rustc nightly feature plus `build-std` and `panic_immediate_abort`, it'll be incredibly 60 KB!

Currently work in progress and only support Windows (Using WinHTTP) and macOS in progress (Using CFNetwork), linux are planned.

## Platform Status

| Name    | Status            | Note                                             |
| ------- | ----------------- | ------------------------------------------------ |
| Windows | Working           |                                                  |
| MacOS   | Working           | Maybe failed sometime or very slow (To be fixed) |
| Linux   | Under Development |                                                  |

## Features

- `async_t_boxed`: Use `async-trait` instead of `async-t`, which requires nightly rustc but with zero-cost. Default is enabled.
- ~~`serde`: Can give you the ability of send/receive json data without manually call `serde_json`. Default is disabled.~~

## Compilation binary size comparison

> Note: the size optimization argument is: `cargo +nightly run --release -Z build-std=core,alloc,std,panic_abort -Z build-std-features=panic_immediate_abort --target [TARGET] --example [EXAMPLE]` and some configuration in [`Cargo.toml`](./Cargo.toml)

| Name                                                | Windows (x86_64) | Windows (i686) | Windows (aarch64) | macOS (x86_64) | macOS (aarch64) | Linux (x86_64) |
| --------------------------------------------------- | ---------------: | -------------: | ----------------: | -------------: | --------------: | -------------: |
| example `https`                                     |          397,824 |        284,160 |           296,960 |      1,044,008 |       1,250,051 |            WIP |
| example `https` release                             |          181,760 |        187,904 |           200,192 |        596,040 |         570,515 |              / |
| example `https` release with size optimization      |           60,416 |         52,224 |            59,392 |         88,736 |          89,048 |              / |
| example `parallel`                                  |          520,704 |        376,320 |           393,216 |      1,321,192 |       1,573,398 |              / |
| example `parallel` release                          |          195,072 |        211,456 |           229,888 |        627,760 |         619,702 |              / |
| example `parallel` release with size optimization   |           66,560 |         58,880 |            66,560 |        105,208 |         105,784 |              / |
| example `sequential`                                |          402,432 |        289,280 |           302,080 |      1,041,424 |       1,244,408 |              / |
| example `sequential` release                        |          185,344 |        191,488 |           203,264 |        593,072 |         566,888 |              / |
| example `sequential` release with size optimization |           62,464 |         54,784 |            60,928 |         88,776 |          89,224 |              / |
