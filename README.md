# Async Lightweight HTTP Client (aka ALHC)

What if we need async but also lightweight http client without using such a large library like `isahc` or `surf`?

ALHC is a async http client library that using System library to reduce binary size and provide async request feature.

Our little parrell request example [`https`](./examples/https.rs) with release build can be 204 KB, which is smaller than `tinyget`'s `http` example. If we use rustc nightly feature plus `build-std` and `panic_immediate_abort`, it'll be incredibly 59 KB!

Currently work in progress and only support Windows (Using WinHTTP), other platform (Linux and macOS) are planned.
