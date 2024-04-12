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

Currently work in progress and support Windows (Using WinHTTP) and unix-like system (including macOS) (Using System libcurl by wraping [`isahc`](https://github.com/sagebind/isahc) crate (Will be replaced by simplier `curl` crate binding)).

## Platform Status

| Name    | Status  | Note                                                                 |
| ------- | ------- | -------------------------------------------------------------------- |
| Windows | Working | Maybe unstable (To be optimized)                                     |
| macOS   | Working | Simple wrapper of [`isahc`](https://github.com/sagebind/isahc) crate |
| Linux   | Working | Simple wrapper of [`isahc`](https://github.com/sagebind/isahc) crate |

## Features

- `async_t_boxed`: Use `async-trait` instead of `async-t`, which requires 1.75+ version of rustc but with zero-cost. Default is disabled.
- `serde`: Can give you the ability of send/receive json data without manually call `serde_json`. Default is disabled.
- `anyhow`: Use `Result` type from `anyhow` crate instead `Result<T, Box<dyn std::error::Error>>`. Default is disabled.

## Minimum binary size on unix-like platform guideline

For Unix-like system like linux or macOS which have builtin libcurl on almost all desktop version, you have to install all the development packages that `curl` crate needs to dynamic link these libraries. For an example, on Ubuntu, you need to install `libcurl4-openssl-dev` and `zlib1g-dev` for a dynamic linkage. Else `curl` crate will build from source and static link them and heavily impact binary size.

## Compilation binary size comparison

> Note: the size optimization argument is: `cargo +nightly run --release -Z build-std=core,alloc,std,panic_abort -Z build-std-features=panic_immediate_abort --target [TARGET] --example [EXAMPLE]` and some configuration in [`Cargo.toml`](./Cargo.toml)

| Name                                                | Windows (x86_64) | Windows (i686) | Windows (aarch64) | macOS (x86_64) | macOS (aarch64) | Linux (x86_64) |
| --------------------------------------------------- | ---------------: | -------------: | ----------------: | -------------: | --------------: | -------------: |
| example `https`                                     |          468,992 |        402,944 |           296,960 |      4,078,936 |       4,395,400 |     18,051,064 |
| example `https` release                             |          181,248 |        162,816 |           200,192 |        729,304 |         719,192 |        850,704 |
| example `https` release with size optimization      |           75,264 |         66,048 |            59,392 |        444,680 |         453,064 |        465,480 |
| example `parallel`                                  |          571,904 |        486,912 |           393,216 |      4,250,296 |       4,572,120 |     19,612,824 |
| example `parallel` release                          |          190,464 |        170,496 |           229,888 |        737,536 |         735,752 |        862,992 |
| example `parallel` release with size optimization   |           80,896 |         71,680 |            66,560 |        452,912 |         453,112 |        469,576 |
| example `sequential`                                |          472,064 |        405,504 |           302,080 |      4,069,368 |       4,373,352 |     18,048,624 |
| example `sequential` release                        |          182,784 |        164,864 |           203,264 |        729,296 |         719,176 |        850,704 |
| example `sequential` release with size optimization |           76,800 |         68,096 |            60,928 |        448,776 |         453,064 |        465,480 |
