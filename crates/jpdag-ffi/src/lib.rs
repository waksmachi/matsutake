//! Bridge that exposes `jpdag` to Dart. `flutter_rust_bridge_codegen generate` writes
//! `frb_generated.rs` from the `api` module.

pub mod api;
// The codegen formats this file itself, in another style.
#[rustfmt::skip]
mod frb_generated;
